//! The spots stream (contract section 9): one long `GET <spots_url>` read as Server-Sent Events.
//!
//! [`run`] is the task. It keeps the stream open while its [`StreamHost`] says it should be (the
//! DX cluster switch is on, a token is stored and `GET /me` listed `spots:read`) and closes it as
//! soon as that stops holding. Every failure but 401 and 403 reconnects after the backoff of
//! section 9.5; the count starts over once a stream has said `hello` and stayed open a minute.
//!
//! The stream has its own HTTP client: the API client's 30-second total timeout would cut every
//! stream off. This one only limits connecting, and a minute without a byte (the server sends a
//! keepalive comment every 20 seconds) counts as a broken stream.

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeDelta, Utc};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use tokio::sync::watch;
use tokio::time::Instant;

use super::sse::SseParser;
use super::{Hello, Spot};
use crate::state::ClusterUpstream;
use crate::upstream::client::{retry_after, user_agent, ClientError};
use crate::upstream::queue::Backoff;
use crate::upstream::token_store::Token;

/// A stream silent this long is closed and opened again (contract 9.5).
pub const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(60);
/// A stream open this long after `hello` resets the backoff (contract 9.5).
pub const STABLE_AFTER: Duration = Duration::from_secs(60);
/// The longest wait for the stream's response headers.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// How the stream behaves. [`StreamConfig::new`] has the contract's values; tests shorten them.
#[derive(Clone, Debug)]
pub struct StreamConfig {
    pub relay_version: String,
    pub backoff: Backoff,
    pub keepalive_timeout: Duration,
    pub stable_after: Duration,
    pub connect_timeout: Duration,
}

impl StreamConfig {
    pub fn new(relay_version: impl Into<String>) -> Self {
        Self {
            relay_version: relay_version.into(),
            backoff: Backoff::DEFAULT,
            keepalive_timeout: KEEPALIVE_TIMEOUT,
            stable_after: STABLE_AFTER,
            connect_timeout: CONNECT_TIMEOUT,
        }
    }
}

/// What the stream should be doing now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Closed: the switch is off, no token is stored, or the token lacks `spots:read`.
    Off,
    /// Open at `url` with `token`.
    Open { token: Token, url: String },
}

/// The engine's side of the stream. Every method returns at once.
pub trait StreamHost: Send + Sync + 'static {
    /// Read again whenever the `changes` channel given to [`run`] moves.
    fn target(&self) -> Target;
    fn status(&self, status: ClusterUpstream);
    fn hello(&self, hello: Hello);
    fn spot(&self, spot: Spot);
    /// 401 `bad_token`: sign out (when `token` is still the stored one).
    fn refused(&self, token: &Token);
    /// 403 `insufficient_scope`: never reconnect with `token`; the user signs in again.
    fn scope_missing(&self, token: &Token);
}

/// True when the token may be sent to `url`: https, or http to this computer (contract 5).
pub fn stream_url_ok(url: &str) -> bool {
    crate::upstream::auth::browser_url_ok(url)
}

/// Why a stream ended.
#[derive(Debug, PartialEq, Eq)]
enum End {
    /// Reconnect after the backoff, or after `retry_after` when that is longer (429).
    Failed {
        reason: String,
        retry_after: Option<Duration>,
    },
    SignedOut,
    ScopeMissing,
}

impl End {
    fn failed(reason: impl Into<String>) -> Self {
        End::Failed {
            reason: reason.into(),
            retry_after: None,
        }
    }
}

/// The stream's HTTP client. Never follows a redirect, so the token goes only where `GET /me`
/// said.
#[derive(Clone, Debug)]
pub struct StreamClient {
    http: reqwest::Client,
    connect_timeout: Duration,
}

impl StreamClient {
    pub fn new(config: &StreamConfig) -> Result<Self, ClientError> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent(&config.relay_version))
            .connect_timeout(config.connect_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ClientError::Retry {
                reason: format!("couldn't start the HTTP client: {e}"),
            })?;
        Ok(Self {
            http,
            connect_timeout: config.connect_timeout,
        })
    }

    async fn open(&self, url: &str, token: &Token) -> Result<reqwest::Response, End> {
        let request = self
            .http
            .get(url)
            .bearer_auth(token.expose())
            .header(ACCEPT, "text/event-stream");
        let response = match tokio::time::timeout(self.connect_timeout, request.send()).await {
            Err(_) => return Err(End::failed("Long Lines didn't answer")),
            Ok(Err(e)) => {
                tracing::debug!(error = %e.without_url(), "couldn't open the spots stream");
                return Err(End::failed("couldn't reach Long Lines"));
            }
            Ok(Ok(response)) => response,
        };
        match response.status().as_u16() {
            200 => {}
            401 => return Err(End::SignedOut),
            403 => return Err(End::ScopeMissing),
            429 => {
                return Err(End::Failed {
                    reason: "rate limited".into(),
                    retry_after: Some(retry_after(response.headers())),
                })
            }
            s => return Err(End::failed(format!("Long Lines answered with HTTP {s}"))),
        }
        let is_stream = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.trim()
                    .to_ascii_lowercase()
                    .starts_with("text/event-stream")
            });
        if !is_stream {
            return Err(End::failed("Long Lines answered without a stream"));
        }
        Ok(response)
    }
}

/// One connection, from the request to its end. The flag says whether it was stable (said
/// `hello` and stayed open [`StreamConfig::stable_after`]).
async fn session<H: StreamHost>(
    host: &H,
    client: &StreamClient,
    token: &Token,
    url: &str,
    config: &StreamConfig,
) -> (End, bool) {
    let mut response = match client.open(url, token).await {
        Ok(response) => response,
        Err(end) => return (end, false),
    };
    tracing::info!("the spots stream is open");
    let mut parser = SseParser::new();
    let mut hello_at: Option<Instant> = None;
    let stable =
        |hello_at: Option<Instant>| hello_at.is_some_and(|t| t.elapsed() >= config.stable_after);
    loop {
        let chunk = match tokio::time::timeout(config.keepalive_timeout, response.chunk()).await {
            Err(_) => return (End::failed("the spots stream went quiet"), stable(hello_at)),
            Ok(Err(e)) => {
                tracing::debug!(error = %e.without_url(), "the spots stream broke");
                return (End::failed("the spots stream broke"), stable(hello_at));
            }
            Ok(Ok(None)) => return (End::failed("the spots stream ended"), stable(hello_at)),
            Ok(Ok(Some(chunk))) => chunk,
        };
        let events = match parser.feed(&chunk) {
            Ok(events) => events,
            Err(e) => return (End::failed(e.to_string()), stable(hello_at)),
        };
        for event in events {
            match event.event.as_str() {
                "hello" if hello_at.is_none() => {
                    hello_at = Some(Instant::now());
                    host.status(ClusterUpstream::Connected);
                    match serde_json::from_str::<Hello>(&event.data) {
                        Ok(hello) => host.hello(hello),
                        Err(e) => tracing::debug!(error = %e, "unreadable hello event"),
                    }
                }
                "spot" => {
                    if let Some(spot) = Spot::from_data(&event.data) {
                        host.spot(spot);
                    }
                }
                // Unknown events are ignored (contract 9.3), as is a second hello.
                _ => {}
            }
        }
    }
}

/// Keeps the stream open while the host wants it. Returns when the `changes` sender is dropped.
pub async fn run<H: StreamHost>(
    host: Arc<H>,
    client: StreamClient,
    config: StreamConfig,
    mut changes: watch::Receiver<u64>,
) {
    let mut failures: u32 = 0;
    loop {
        changes.borrow_and_update();
        let target = host.target();
        let Target::Open { token, url } = target.clone() else {
            failures = 0;
            host.status(ClusterUpstream::Off);
            if changes.changed().await.is_err() {
                return;
            }
            continue;
        };
        host.status(ClusterUpstream::Connecting);
        let ended = {
            let session = session(&*host, &client, &token, &url, &config);
            tokio::pin!(session);
            loop {
                tokio::select! {
                    ended = &mut session => break Some(ended),
                    changed = changes.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        if host.target() != target {
                            break None;
                        }
                    }
                }
            }
        };
        // The target changed: the stream is dropped (closed) and the new target read at once.
        let Some((end, stable)) = ended else {
            tracing::info!("closed the spots stream");
            failures = 0;
            continue;
        };
        if stable {
            failures = 0;
        }
        let wait = match end {
            End::SignedOut => {
                host.refused(&token);
                continue;
            }
            End::ScopeMissing => {
                tracing::info!("the token can't read spots; sign in again to use the DX cluster");
                host.scope_missing(&token);
                continue;
            }
            End::Failed {
                reason,
                retry_after,
            } => {
                failures = failures.saturating_add(1);
                let backoff = config.backoff.delay(failures);
                let wait = retry_after.map_or(backoff, |r| r.max(backoff));
                tracing::info!(%reason, seconds = wait.as_secs(), "reconnecting to the spots stream");
                wait
            }
        };
        host.status(ClusterUpstream::Retrying {
            at: Utc::now() + TimeDelta::from_std(wait).unwrap_or(TimeDelta::minutes(5)),
        });
        let deadline = Instant::now() + wait;
        loop {
            tokio::select! {
                () = tokio::time::sleep_until(deadline) => break,
                changed = changes.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    if host.target() != target {
                        failures = 0;
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_goes_only_to_safe_urls() {
        assert!(stream_url_ok("https://spots.longlines.io/v2/spots"));
        assert!(stream_url_ok("http://127.0.0.1:8081/v2/spots"));
        assert!(stream_url_ok("http://localhost:8081/v2/spots"));
        assert!(stream_url_ok("http://[::1]:8081/v2/spots"));
        assert!(!stream_url_ok("http://spots.longlines.io/v2/spots"));
        assert!(!stream_url_ok("ftp://spots.longlines.io/"));
        assert!(!stream_url_ok(""));
    }

    #[test]
    fn contract_timings() {
        let c = StreamConfig::new("0.2.0");
        assert_eq!(c.keepalive_timeout, Duration::from_secs(60));
        assert_eq!(c.stable_after, Duration::from_secs(60));
        assert_eq!(c.backoff.delay(1), Duration::from_secs(5));
        assert_eq!(c.backoff.delay(7), Duration::from_secs(300));
    }
}
