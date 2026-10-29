//! The outgoing queue, in SQLite so a restart loses nothing.
//!
//! Items leave in the order they arrived: a batch is always the oldest items, and when the oldest
//! is backing off, everything behind it waits too. A failed batch backs off together,
//! exponentially from 5 seconds to 5 minutes. Upserts that pass the 30-minute window while
//! waiting are expired; deletes never age out.
//!
//! A row this build can't read (written by a newer version, or damaged) is moved to the
//! `outbox_unreadable` table and logged, so it never blocks the rows behind it.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::{Op, QueuedContact};
use crate::freshness::{classify, Freshness};
use crate::n1mm::parse_timestamp;

/// The schema version in `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 1;

/// Exponential backoff between failed sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backoff {
    pub base: Duration,
    pub max: Duration,
}

impl Backoff {
    /// 5 s, 10 s, 20 s ... capped at 5 minutes.
    pub const DEFAULT: Backoff = Backoff {
        base: Duration::from_secs(5),
        max: Duration::from_secs(300),
    };

    /// The wait after the `attempts`-th failure (1-based).
    pub fn delay(&self, attempts: u32) -> Duration {
        let exp = attempts.saturating_sub(1).min(20);
        self.base.saturating_mul(1u32 << exp).min(self.max)
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One waiting item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueItem {
    pub seq: i64,
    pub contact: QueuedContact,
    pub attempts: u32,
    pub next_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

/// What the sender should do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NextBatch {
    Empty,
    /// The oldest item is backing off until then.
    WaitUntil(DateTime<Utc>),
    Ready(Vec<QueueItem>),
}

/// Why an item left the queue without being sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropReason {
    /// An upsert past the 30-minute window.
    TooOld,
    /// More than 5 minutes ahead of the clock.
    ClockSkew,
}

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("queue database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("queue row {seq} is unreadable: {source}")]
    Row { seq: i64, source: serde_json::Error },
    #[error("couldn't create the queue folder: {0}")]
    Io(#[from] std::io::Error),
}

/// The on-disk outbox.
#[derive(Debug)]
pub struct Queue {
    conn: Mutex<Connection>,
    backoff: Backoff,
}

fn ms(t: DateTime<Utc>) -> i64 {
    t.timestamp_millis()
}

fn from_ms(v: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(v).unwrap_or_default()
}

impl Queue {
    /// Opens (creating if needed) the queue at `path`.
    pub fn open(path: &Path, backoff: Backoff) -> Result<Self, QueueError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?, backoff)
    }

    /// A queue that lives only as long as this value (tests).
    pub fn open_in_memory(backoff: Backoff) -> Result<Self, QueueError> {
        Self::init(Connection::open_in_memory()?, backoff)
    }

    fn init(conn: Connection, backoff: Backoff) -> Result<Self, QueueError> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            "create table if not exists outbox (
                seq integer primary key autoincrement,
                external_id text not null,
                op text not null,
                body text not null,
                created_at integer not null,
                attempts integer not null default 0,
                next_at integer not null,
                last_error text
            );
            create index if not exists outbox_external_id on outbox (external_id);
            create table if not exists outbox_unreadable (
                seq integer primary key,
                body text not null,
                error text not null,
                moved_at integer not null
            );",
        )?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
            backoff,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Adds an item at the back. Returns its sequence number.
    pub fn enqueue(&self, contact: &QueuedContact, now: DateTime<Utc>) -> Result<i64, QueueError> {
        let body =
            serde_json::to_string(contact).map_err(|source| QueueError::Row { seq: 0, source })?;
        let conn = self.lock();
        conn.execute(
            "insert into outbox (external_id, op, body, created_at, next_at) values (?1, ?2, ?3, ?4, ?4)",
            params![contact.external_id, contact.op.as_str(), body, ms(now)],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Every waiting item, oldest first.
    pub fn pending(&self) -> Result<Vec<QueueItem>, QueueError> {
        self.select(
            "select seq, body, attempts, next_at, last_error from outbox order by seq",
            params![],
        )
    }

    pub fn len(&self) -> Result<usize, QueueError> {
        let n: i64 = self
            .lock()
            .query_row("select count(*) from outbox", [], |r| r.get(0))?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    pub fn is_empty(&self) -> Result<bool, QueueError> {
        Ok(self.len()? == 0)
    }

    /// Up to `limit` of the oldest items, if the oldest is due.
    pub fn next_batch(&self, now: DateTime<Utc>, limit: usize) -> Result<NextBatch, QueueError> {
        let head: Option<i64> = self
            .lock()
            .query_row("select next_at from outbox order by seq limit 1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(head_next_at) = head else {
            return Ok(NextBatch::Empty);
        };
        if head_next_at > ms(now) {
            return Ok(NextBatch::WaitUntil(from_ms(head_next_at)));
        }
        if limit == 0 {
            return Ok(NextBatch::WaitUntil(now));
        }
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let items = self.select(
            "select seq, body, attempts, next_at, last_error from outbox order by seq limit ?1",
            params![limit],
        )?;
        Ok(NextBatch::Ready(items))
    }

    /// Removes items that got a final answer.
    pub fn remove(&self, seqs: &[i64]) -> Result<(), QueueError> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for seq in seqs {
            tx.execute("delete from outbox where seq = ?1", params![seq])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Removes the waiting items for `external_id` with operation `op` (an N1MM edit folds its
    /// delete into the replace that follows). Returns their sequence numbers.
    pub fn remove_waiting(&self, external_id: &str, op: Op) -> Result<Vec<i64>, QueueError> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("delete from outbox where external_id = ?1 and op = ?2 returning seq")?;
        let seqs = stmt
            .query_map(params![external_id, op.as_str()], |r| r.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(seqs)
    }

    /// A failed send: one more attempt, and the batch backs off together. Returns the next try.
    pub fn mark_failed(
        &self,
        seqs: &[i64],
        error: &str,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, QueueError> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut next = now;
        for seq in seqs {
            let attempts: Option<u32> = tx
                .query_row(
                    "select attempts from outbox where seq = ?1",
                    params![seq],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(attempts) = attempts else { continue };
            let attempts = attempts.saturating_add(1);
            let delay = TimeDelta::from_std(self.backoff.delay(attempts)).unwrap_or(TimeDelta::MAX);
            let at = now.checked_add_signed(delay).unwrap_or(now);
            next = next.max(at);
            tx.execute(
                "update outbox set attempts = ?2, next_at = ?3, last_error = ?4 where seq = ?1",
                params![seq, attempts, ms(at), error],
            )?;
        }
        tx.commit()?;
        Ok(next)
    }

    /// Holds items until `until` without counting an attempt (a 429).
    pub fn defer(&self, seqs: &[i64], until: DateTime<Utc>, error: &str) -> Result<(), QueueError> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for seq in seqs {
            tx.execute(
                "update outbox set next_at = ?2, last_error = ?3 where seq = ?1",
                params![seq, ms(until), error],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Removes items that may no longer be sent: upserts more than 30 minutes old, and anything
    /// more than 5 minutes ahead of the clock. Returns them with the reason.
    pub fn expire(&self, now: DateTime<Utc>) -> Result<Vec<(QueueItem, DropReason)>, QueueError> {
        let mut dropped = Vec::new();
        for item in self.pending()? {
            let Some(logged) = parse_timestamp(&item.contact.timestamp_utc) else {
                continue;
            };
            let reason = match (classify(logged, now), item.contact.op) {
                (Freshness::Future, _) => DropReason::ClockSkew,
                (Freshness::TooOld, Op::Upsert) => DropReason::TooOld,
                _ => continue,
            };
            dropped.push((item, reason));
        }
        let seqs: Vec<i64> = dropped.iter().map(|(i, _)| i.seq).collect();
        self.remove(&seqs)?;
        Ok(dropped)
    }

    /// The rows `sql` selects, oldest first. Rows that can't be read are set aside, not returned.
    fn select(&self, sql: &str, args: impl rusqlite::Params) -> Result<Vec<QueueItem>, QueueError> {
        let mut conn = self.lock();
        let rows = {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map(args, |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u32>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let mut items = Vec::with_capacity(rows.len());
        let mut unreadable = Vec::new();
        for (seq, body, attempts, next_at, last_error) in rows {
            match serde_json::from_str(&body) {
                Ok(contact) => items.push(QueueItem {
                    seq,
                    contact,
                    attempts,
                    next_at: from_ms(next_at),
                    last_error,
                }),
                Err(source) => unreadable.push((seq, body, source)),
            }
        }
        if !unreadable.is_empty() {
            set_aside(&mut conn, unreadable)?;
        }
        Ok(items)
    }
}

/// Moves unreadable rows out of the outbox, keeping their bodies for a later version to read.
fn set_aside(
    conn: &mut Connection,
    rows: Vec<(i64, String, serde_json::Error)>,
) -> Result<(), QueueError> {
    let tx = conn.transaction()?;
    let now = ms(Utc::now());
    for (seq, body, error) in rows {
        tracing::error!(seq, error = %error, "set aside a queued contact this version can't read");
        tx.execute(
            "insert or replace into outbox_unreadable (seq, body, error, moved_at) values (?1, ?2, ?3, ?4)",
            params![seq, body, error.to_string(), now],
        )?;
        tx.execute("delete from outbox where seq = ?1", params![seq])?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::n1mm::ContactKind;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, 21, 14, 0).unwrap()
    }

    fn contact(id: &str, op: Op, logged: DateTime<Utc>) -> QueuedContact {
        QueuedContact {
            external_id: id.into(),
            op,
            kind: if op == Op::Delete {
                ContactKind::Delete
            } else {
                ContactKind::Info
            },
            mycall: "KK4PWJ".into(),
            call: "W1AW".into(),
            frequency_hz: Some(14_025_000),
            band: Some("20m".into()),
            mode: "CW".into(),
            timestamp_utc: logged.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            logger_app: Some("N1MM".into()),
        }
    }

    fn quick() -> Backoff {
        Backoff::DEFAULT
    }

    #[test]
    fn backoff_doubles_to_five_minutes() {
        let b = Backoff::DEFAULT;
        let secs: Vec<u64> = (1..=9).map(|a| b.delay(a).as_secs()).collect();
        assert_eq!(secs, vec![5, 10, 20, 40, 80, 160, 300, 300, 300]);
        assert_eq!(b.delay(0).as_secs(), 5);
        assert_eq!(b.delay(u32::MAX).as_secs(), 300);
    }

    #[test]
    fn batches_are_oldest_first_and_limited() {
        let q = Queue::open_in_memory(quick()).unwrap();
        assert_eq!(q.next_batch(now(), 100).unwrap(), NextBatch::Empty);
        for i in 0..5 {
            q.enqueue(&contact(&format!("id{i}"), Op::Upsert, now()), now())
                .unwrap();
        }
        let NextBatch::Ready(items) = q.next_batch(now(), 3).unwrap() else {
            panic!()
        };
        let ids: Vec<_> = items
            .iter()
            .map(|i| i.contact.external_id.as_str())
            .collect();
        assert_eq!(ids, ["id0", "id1", "id2"]);
        q.remove(&[items[0].seq, items[1].seq]).unwrap();
        assert_eq!(q.len().unwrap(), 3);
        let NextBatch::Ready(items) = q.next_batch(now(), 100).unwrap() else {
            panic!()
        };
        assert_eq!(items[0].contact.external_id, "id2");
    }

    #[test]
    fn a_failed_head_holds_everything_behind_it() {
        let q = Queue::open_in_memory(quick()).unwrap();
        let first = q.enqueue(&contact("a", Op::Upsert, now()), now()).unwrap();
        let next = q.mark_failed(&[first], "down", now()).unwrap();
        assert_eq!(next, now() + TimeDelta::seconds(5));
        q.enqueue(&contact("b", Op::Upsert, now()), now()).unwrap();
        assert_eq!(
            q.next_batch(now(), 100).unwrap(),
            NextBatch::WaitUntil(next)
        );
        let NextBatch::Ready(items) = q.next_batch(next, 100).unwrap() else {
            panic!()
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].attempts, 1);
        assert_eq!(items[0].last_error.as_deref(), Some("down"));
        let again = q.mark_failed(&[first], "down", next).unwrap();
        assert_eq!(again, next + TimeDelta::seconds(10));
    }

    #[test]
    fn defer_does_not_count_an_attempt() {
        let q = Queue::open_in_memory(quick()).unwrap();
        let seq = q.enqueue(&contact("a", Op::Upsert, now()), now()).unwrap();
        let until = now() + TimeDelta::seconds(30);
        q.defer(&[seq], until, "rate limited").unwrap();
        assert_eq!(
            q.next_batch(now(), 10).unwrap(),
            NextBatch::WaitUntil(until)
        );
        assert_eq!(q.pending().unwrap()[0].attempts, 0);
    }

    #[test]
    fn expire_drops_old_upserts_and_future_items_but_keeps_old_deletes() {
        let q = Queue::open_in_memory(quick()).unwrap();
        let old = now() - TimeDelta::minutes(31);
        q.enqueue(&contact("old-upsert", Op::Upsert, old), now())
            .unwrap();
        q.enqueue(&contact("old-delete", Op::Delete, old), now())
            .unwrap();
        q.enqueue(&contact("fresh", Op::Upsert, now()), now())
            .unwrap();
        q.enqueue(
            &contact("future", Op::Delete, now() + TimeDelta::minutes(6)),
            now(),
        )
        .unwrap();
        let dropped = q.expire(now()).unwrap();
        let got: Vec<_> = dropped
            .iter()
            .map(|(i, r)| (i.contact.external_id.as_str(), *r))
            .collect();
        assert_eq!(
            got,
            [
                ("old-upsert", DropReason::TooOld),
                ("future", DropReason::ClockSkew)
            ]
        );
        let left: Vec<_> = q
            .pending()
            .unwrap()
            .into_iter()
            .map(|i| i.contact.external_id)
            .collect();
        assert_eq!(left, ["old-delete", "fresh"]);
    }

    #[test]
    fn an_upsert_ages_out_while_waiting() {
        let q = Queue::open_in_memory(quick()).unwrap();
        q.enqueue(&contact("a", Op::Upsert, now()), now()).unwrap();
        assert!(q.expire(now() + TimeDelta::minutes(30)).unwrap().is_empty());
        let dropped = q.expire(now() + TimeDelta::minutes(31)).unwrap();
        assert_eq!(dropped.len(), 1);
        assert!(q.is_empty().unwrap());
    }

    #[test]
    fn remove_waiting_by_id_and_op() {
        let q = Queue::open_in_memory(quick()).unwrap();
        let del = q.enqueue(&contact("a", Op::Delete, now()), now()).unwrap();
        q.enqueue(&contact("a", Op::Upsert, now()), now()).unwrap();
        q.enqueue(&contact("b", Op::Delete, now()), now()).unwrap();
        assert_eq!(q.remove_waiting("a", Op::Delete).unwrap(), vec![del]);
        assert!(q.remove_waiting("a", Op::Delete).unwrap().is_empty());
        assert_eq!(q.len().unwrap(), 2);
    }

    #[test]
    fn unreadable_rows_are_set_aside() {
        let q = Queue::open_in_memory(quick()).unwrap();
        q.enqueue(&contact("a", Op::Upsert, now()), now()).unwrap();
        q.lock()
            .execute(
                "insert into outbox (external_id, op, body, created_at, next_at) values ('bad', 'upsert', '{\"kind\": \"spot\"}', 0, 0)",
                [],
            )
            .unwrap();
        q.enqueue(&contact("b", Op::Upsert, now()), now()).unwrap();
        let ids: Vec<_> = q
            .pending()
            .unwrap()
            .into_iter()
            .map(|i| i.contact.external_id)
            .collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(q.len().unwrap(), 2);
        let kept: i64 = q
            .lock()
            .query_row("select count(*) from outbox_unreadable", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 1);
        let NextBatch::Ready(items) = q.next_batch(now(), 100).unwrap() else {
            panic!()
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("queue.sqlite3");
        {
            let q = Queue::open(&path, quick()).unwrap();
            let seq = q.enqueue(&contact("a", Op::Upsert, now()), now()).unwrap();
            q.enqueue(&contact("b", Op::Delete, now()), now()).unwrap();
            q.mark_failed(&[seq], "down", now()).unwrap();
        }
        let q = Queue::open(&path, quick()).unwrap();
        let items = q.pending().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].contact, contact("a", Op::Upsert, now()));
        assert_eq!(items[0].attempts, 1);
        assert_eq!(items[1].contact.op, Op::Delete);
        // New items keep their order after the old ones.
        q.enqueue(&contact("c", Op::Upsert, now()), now()).unwrap();
        let ids: Vec<_> = q
            .pending()
            .unwrap()
            .into_iter()
            .map(|i| i.contact.external_id)
            .collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }
}
