# Code review: Long Lines Relay

Reviewed at `1ca8d4a` (main), 2026-10-28. Scope: bugs, security, error handling and dead code.
Style is out of scope.

Severity: **High**: data loss or the app stops working, reachable in normal operation.
**Medium**: wrong behaviour or misleading UI that a user will hit. **Low**: an edge case, hardening
or dead code.

Findings marked **(verified)** were reproduced with a throwaway integration test, since deleted.
The existing suite (`cargo test -p relay-core`) passes.

**Status:** #1 to #7 (every High and Medium) are fixed, with tests in
`crates/relay-core/tests/resilience.rs`, `upstream/queue.rs`, `listeners/udp.rs` and the page's
screen tests. Each section below ends with what changed. #8 to #16 are open. File and line
references are as of `1ca8d4a`; the fixes were rebased onto the Phase 2 work (tabs, spots stream,
DX cluster) that landed after it.

## Structure

| Area | Files | Role |
| --- | --- | --- |
| Core: input | `listeners/udp.rs`, `source_filter.rs`, `n1mm.rs` | Binds UDP `0.0.0.0:<port>`, drops non-local sources, parses N1MM XML |
| Core: checks | `callsign.rs`, `freshness.rs`, `bands.rs`, `external_id.rs` | Contract rules shared with the server |
| Core: orchestration | `engine.rs`, `state.rs`, `bus.rs`, `status.rs` | Pipeline task, sender task, pairing, UI state |
| Core: upstream | `upstream/{client,queue,auth,token_store}.rs` | HTTP client, SQLite outbox, device pairing, credential store |
| Core: support | `settings.rs`, `logging.rs`, `fake_logger.rs`, `bin/fake_logger.rs` | Settings file, rolling log, dev tool |
| Shell | `src-tauri/src/{lib,commands,view,tray,updates}.rs` | Tauri wiring, commands, updater, autostart |
| Window | `src/**` | React/MUI page; `useRelay` mirrors the shell's `state` event |
| CI | `.github/workflows/{ci,release}.yml` | Test, bundle, signed release |

Data flow: UDP datagram → `pipeline` task → parse and check → `Queue::enqueue` (SQLite) → `sender`
task → `POST /contacts` → row status → bus → shell bridge → `state` event → page.

## Summary

| # | Severity | Where | Finding |
| --- | --- | --- | --- |
| 1 | High | `engine.rs:851` | A large `Retry-After` panics the sender task; nothing is sent again until restart **(verified)**. **Fixed** |
| 2 | High | `queue.rs:306`, `engine.rs:175`, `src-tauri/src/lib.rs:170` | One unreadable queue row stops the app from launching and wedges the queue **(verified)**. **Fixed** |
| 3 | Medium | `PortField.tsx:17`, `engine.rs:223` | No way to recover after "Port in use" without changing ports or restarting. **Fixed** |
| 4 | Medium | `udp.rs:64` | Restarting on the same port races the aborted task's socket **(verified: 49/50 fail)**. **Fixed** |
| 5 | Medium | `AccountCard.tsx:23` | "Connected" is hard-coded; retrying, rate-limited and dead-sender states are never shown. **Fixed** |
| 6 | Medium | `commands.rs:58`, `App.tsx:23` | A failed sign-out looks like it worked; the token comes back at the next launch. **Fixed** |
| 7 | Medium | `engine.rs:981`, `view.rs:34` | A credential-store read error shows the sign-in screen while a token exists. **Fixed** |
| 8 | Low | `App.tsx:16` | Command errors are only `console.error`ed (`set_port`, `set_autostart`) |
| 9 | Low | `engine.rs:269` | Cancel during sign-in can still store the token; a concurrent start can leak a poll task |
| 10 | Low | `client.rs:288` | The approved token is never shape-checked; `Token::is_well_formed` is dead |
| 11 | Low | `udp.rs:56`, `source_filter.rs:62` | The local-only rule trusts the source IP; a LAN host can spoof it |
| 12 | Low | `udp.rs:110` | The receive loop spins with no backoff on a persistent error |
| 13 | Low | `settings.rs:67` | An older build saving a newer settings file drops its unknown fields |
| 14 | Low | `settings.rs:75` | Settings are not fsynced before the rename |
| 15 | Low | `release.yml:91` | The updater signing key is in the environment while npm dependency code runs |
| 16 | Low | several | Dead code |

---

## relay-core: engine (`crates/relay-core/src/engine.rs`)

### 1. A large `Retry-After` panics the sender task (High, verified)

- `engine.rs:851-852`: `now + TimeDelta::from_std(retry_after).unwrap_or(...)`
- `engine.rs:1013-1014`: the same pattern on the `GET /me` path
- The cause is in `upstream/client.rs:375-381`: `retry_after()` accepts any `u64`.

`TimeDelta::from_std` only fails above about 9.2e15 seconds, but `DateTime + TimeDelta` panics
past chrono's maximum date (about 8e12 seconds from now). A 429 with `Retry-After:
100000000000000` therefore panics inside the `sender` task. Tokio swallows a task's panic, so
the relay keeps receiving, queueing and showing "Waiting to send", but never sends again. The
upstream status stays `Connected`. The probe confirmed it: the panic hit `engine.rs:852`, the
second contact was never requested, and the snapshot still said `Connected`. The server is
trusted over TLS, but a misbehaving proxy or CDN in front of it is enough.

**Fix:** clamp in `retry_after()` (for example `.min(Duration::from_secs(3600))`, which also bounds
`queue.defer` and pairing's `extra_wait`), and use `now.checked_add_signed(..).unwrap_or(now +
1h)` at both call sites. Also consider a supervisor around `sender`/`pipeline` that logs and
respawns a task that panics, so the next bug of this kind cannot silently stop the relay.

**Fixed:** `client.rs` caps `Retry-After` at `MAX_RETRY_AFTER` (1 hour), and `engine.rs` adds the
times with `later()`, which saturates instead of panicking. No task supervisor was added.

### 3. No way back from "Port in use" (Medium)

- `src/components/PortField.tsx:17,32`: submit is disabled when `parsed === port`
- `src/components/FeedCard.tsx:63`: the port-in-use alert reuses that same field
- `engine.rs:223-236`: nothing retries a failed bind

When another program holds 12060 at launch and later releases it, the relay stays deaf on that
port. The user can't retry the same port from the UI. The only remedies are switching the relay
and the logger to another port, or restarting the relay.

**Fix:** retry the bind in the sender's idle tick while the listener is `PortInUse`, or offer a
"Try again" button that calls `set_port` with the current port. Fix #4 first, either way.

**Fixed:** a `rebind` engine task tries the held port again every `PORT_RETRY` (5 s) and logs the
failure once, not every time. `set_port` with the port that's in use also tries again. The
feed card's help text now says the relay takes the port as soon as it's free.

### 7. A credential-store read error is shown as signed out (Medium)

- `engine.rs:981-986`: `tokens.load()` → `Err` → `set_upstream(SignedOut)`
- `engine.rs:176-178`: at launch only `Ok(Some)` sets `Connected`
- `src-tauri/src/view.rs:34`: `signed_in` is derived from `upstream != SignedOut`

When the keyring is locked or unavailable (Linux autostart before GNOME Keyring unlocks, a
denied Keychain prompt), the window shows the sign-in screen even though a token is stored. A
user who signs in again creates a second token on the server, and the first stays valid and
listed. Errors also aren't cached in `CachingTokenStore::load` (`token_store.rs:122-130`), so the
sender makes a blocking credential-store call from an async task on every idle tick.

**Fix:** add an `UpstreamStatus::CredentialStoreUnavailable` (or similar) that keeps the main
screen and explains the problem. Back off the reloads, and move the keyring call onto
`spawn_blocking`.

**Fixed:** the new `UpstreamStatus::CredentialStoreUnavailable` is set at launch and by the sender.
The window stays on the main screen and explains it (see #5), and the relay goes on once the
store can be read. The read error is logged once per episode. Still open: reads retry every
5 s on the idle tick, and the keyring call isn't on `spawn_blocking`.

### 9. Pairing cancel and restart races (Low)

- `engine.rs:269-273`: the attempt check runs before `shared.sign_in(token).await`, and
  `sign_in` saves the token first (`engine.rs:415`). A Cancel that lands during that await
  doesn't undo the save, so the sender then signs the user in after they cancelled.
- `engine.rs:265-290`: the task handle is stored without re-checking `pairing_attempt`. If two
  `start_sign_in` calls overlap, the earlier task can be stored after the later one aborted it.
  It then keeps polling the server until its code expires.

**Fix:** re-check the attempt after `wait_for_token` returns and before `tokens.save`, and do the
save and the check under one lock. Re-check the attempt before storing the handle, and abort the
new task if it is stale.

## relay-core: queue (`crates/relay-core/src/upstream/queue.rs`)

### 2. One unreadable row stops the app from launching and wedges the queue (High, verified)

- `queue.rs:306-318`: `select()` fails the whole result if any row's `body` doesn't deserialize
- `engine.rs:175`: `shared.restore_rows()?` makes `Engine::start` fail
- `src-tauri/src/lib.rs:170`: `block_on(Engine::start(config))?` fails `setup`
- `src-tauri/src/lib.rs:234`: `.expect("Long Lines Relay couldn't start")` then panics. A
  Windows release build has no console (`src-tauri/src/main.rs:2`), so the user sees nothing at all.

With the probe's row inserted, `Engine::start` returned `Queue(Row { seq: 1, ... missing field
external_id })`. A row can become unreadable through a downgrade after a future version changes
`QueuedContact` (a new `ContactKind`, a new required field) or through disk damage. Even without
the startup failure, `expire()` (`engine.rs:775`) and `next_batch()` (`engine.rs:1048`) would
fail on every loop forever.

**Fix:** in `select()`, log and delete (or move to a `dead` table) any row that fails to
deserialize, and return the rest. In `setup`, don't make a queue failure fatal: move the file
aside, open a fresh queue and log it. If launch really can't continue, show a native dialog
instead of panicking.

**Fixed:** `select()` moves rows it can't read into a new `outbox_unreadable` table (keeping the
body), logs them, and returns the rest. In the shell, a queue that can't be opened at all is
renamed to `queue.sqlite3.unreadable-<time>` (with its WAL files), and the relay starts with a
new queue. No native dialog was added for the case where even that fails.

## relay-core: listener (`crates/relay-core/src/listeners/udp.rs`, `source_filter.rs`)

### 4. Restarting on the same port races the aborted socket (Medium, verified)

- `udp.rs:64-67`: `task.abort()` then immediately `bind(self.port)`

`JoinHandle::abort` only schedules cancellation, so the old `UdpSocket` (owned by the task's
future) is still bound when the new bind runs. The probe called `start()` 50 times on one port
and got `PortInUse` 49 times. The relay was reporting its own socket as "another program". The
UI can't trigger it today (see #3), but any "Try again" or same-port `set_port` (`engine.rs:225`
calls `stop()` and then binds) will hit it.

**Fix:** keep the socket in `Running` as an `Arc<UdpSocket>`, so `stop()` drops the last strong
reference synchronously after the abort. Alternatively, make `start`/`set_port` async and await
the aborted handle before rebinding. A third option is to keep the existing socket when the port
hasn't changed.

**Fixed:** `start()` keeps the socket while its task is running, and the new
`UdpListener::shutdown()` awaits the aborted task, so the socket is closed when it returns.
`Engine::set_port` is now async: it does nothing when the listener is already on that port,
otherwise it awaits `shutdown()` before binding.

### 11. The local-only rule trusts the source IP (Low, security)

- `udp.rs:56`: binds `0.0.0.0`
- `source_filter.rs:62-78`: accepts any datagram whose source is loopback or one of this
  machine's interface addresses

UDP source addresses are trivially forged on a LAN. Another host on the same network can send a
datagram claiming to come from this computer's LAN address, and the relay will post it as a
spot. The attacker chooses `mycall`, so the callsign check doesn't help. The README's privacy
note ("only listens for packets from this computer") overstates this.

**Fix:** if the supported loggers send to `127.0.0.1`, bind `127.0.0.1` by default and make
`0.0.0.0` an opt-in setting for broadcast setups. Loopback can't be spoofed from off-host. At
minimum, document the limit.

### 12. The receive loop spins on a persistent error (Low)

- `udp.rs:110-118`: `Err(e) => { debug!(..); continue; }`

That suits the transient Windows `WSAECONNRESET` case, but a persistent error would pin a core
and flood the debug log.

**Fix:** count consecutive errors and sleep briefly (for example 100 ms, up to 1 s) after a few.

## relay-core: client and auth (`upstream/client.rs`, `upstream/auth.rs`, `upstream/token_store.rs`)

### 10. The approved token is never shape-checked (Low)

- `client.rs:288-292`: `Token::new(raw.access_token)` stores whatever arrives
- `token_store.rs:29`: `Token::is_well_formed` exists but is used only in its own unit test

An empty token, or one with characters that aren't valid in a header, gets saved to the
keyring. `bearer_auth` then fails while the request is being built, so every request comes back
as `Retry` ("couldn't reach Long Lines") forever, and the user is never signed out.

**Fix:** in `poll_token`, reject a token that isn't well-formed as `ClientError::Rejected`. The
engine then shows `PairingFailure::Refused`.

## relay-core: settings (`crates/relay-core/src/settings.rs`)

### 13. Downgrade loses settings (Low)

- `settings.rs:67-77`: `save` writes only known fields and forces `version = 1`

The module doc (lines 3-4) promises that an older relay can read a newer file. It can, but its
first save (any port or autostart change) silently drops the newer fields, such as phase 2's
`telnet_port`, and rewrites the version.

**Fix:** keep `#[serde(flatten)] extra: serde_json::Map<String, Value>` on `Settings`, and write
`max(SETTINGS_VERSION, loaded.version)`.

### 14. Not fsynced before the rename (Low)

- `settings.rs:75-76`: `fs::write(tmp)` then `fs::rename`

A crash or power loss between the two can leave an empty or partial file on some filesystems.
`load_or_default` then silently resets the port and autostart.

**Fix:** open the temp file, `write_all`, `sync_all`, then rename.

## Shell (`src-tauri`)

### 6. A failed sign-out looks like it worked (Medium, privacy)

- `src-tauri/src/commands.rs:58`: `sign_out` returns `Err` when the keyring delete fails
- `crates/relay-core/src/upstream/token_store.rs:141-145`: `CachingTokenStore::delete` forgets
  the token in memory regardless
- `src/App.tsx:23`: the page only `console.error`s the error

The window switches to signed-out (the engine published `SignedOut`), but the token is still in
the OS credential store. At the next launch the relay is signed in again. On a shared shack
computer, that defeats the point of signing out.

**Fix:** show the error ("Couldn't remove the sign-in from this computer's credential store")
and retry the delete in the background until it succeeds.

**Fixed:** a failed delete sets `sign_out_incomplete` (in the snapshot and in a
`sign-out-pending` file next to the queue). While it's set, the engine never uses the stored
token, this run or the next (neither the sender nor the Phase 2 spots stream), and the sender
retries the delete. A new sign-in clears it. The
signed-out screen shows a warning, and the `sign_out` command no longer fails.

### 8. Command errors are swallowed (Low)

- `src/App.tsx:16-25`: `run()` only logs a rejection
- `src-tauri/src/commands.rs:83-84`: `set_port` moves the listener before saving. If the save
  fails, the relay listens on the new port until the next launch, then falls back to the old one
  without saying so.
- `commands.rs:96`: when `sync_autostart` fails, the checkbox stays as it was, with no message

**Fix:** have `run()` keep the last error and show it in a snackbar or alert. In `set_port`, save
first, or roll the listener back if the save fails.

## Window (`src`)

### 5. "Connected" is hard-coded (Medium)

- `src/components/AccountCard.tsx:23`: the chip is always `copy.account.connected`
- `src/lib/types.ts:31-35`: `UpstreamStatus` is sent with every state but nothing reads it

When Long Lines can't be reached or answers 429, contacts sit at "Waiting to send" while the
card says "Connected", with no hint why or when the next try is.

**Fix:** drive the chip from `view.upstream` and explain the problem, with its retry time.

**Fixed:** Phase 2 replaced the account card with a header account menu, which carried the same
hard-coded chip (`AppHeader.tsx`). The header chip now reads Connected, Retrying, Paused or
"Can't read sign-in" (`src/lib/upstream.ts`), and `UpstreamAlert` at the top of the Contacts tab
gives the reason and a live countdown to the next try.

## CI (`.github/workflows/release.yml`)

### 15. The updater signing key is exposed to npm dependency code (Low, supply chain)

- `release.yml:91`: `npm ci` runs dependency install scripts
- `release.yml:92-98`: the `tauri-action` step has `TAURI_SIGNING_PRIVATE_KEY` in its env, and
  `beforeBuildCommand` runs `npm run build` (Vite, plugins, every transitive dev dependency)
  inside that step

A compromised npm package would see the key that signs auto-updates for every installed relay.
This is the standard Tauri setup, but it's the most valuable secret in the project.

**Fix:** run `npm ci --ignore-scripts` and `npm run build` in earlier steps without the key.
Then pass `--config '{"build":{"beforeBuildCommand":""}}'` so the signing step only compiles
Rust and signs.

## Dead code (16, Low)

| Where | What | Suggestion |
| --- | --- | --- |
| `engine.rs:212` | `Engine::client()`: no caller | Remove |
| `engine.rs:321` | `Engine::refresh_account()`: no caller in the shell or tests | Remove, or wire it to a "Refresh" action |
| `state.rs:303` | `RelayState::listener()`: no caller | Remove |
| `lib.rs:29` | `RELAY_VERSION`: no caller (the shell passes its own version) | Remove |
| `upstream/queue.rs:167-176` | `Queue::len` / `is_empty`: tests only | `#[cfg(test)]` or leave as API |
| `n1mm.rs:54,191` | `LoggedContact::station_name`: parsed, never used outside tests | Remove, or use it in logs |
| `client.rs:289-291`, `auth.rs:181` | `PollOutcome::Approved::scopes`: discarded | Remove until phase 2, or store it for the re-pair rule (contract 3.1) |
| `token_store.rs:29` | `Token::is_well_formed`: tests only | Use it (see #10) |
| `lib.rs:17` | `pub mod fake_logger` ships in the release library | Gate it behind a `dev-tools` feature used by the bin and the tests |

## Checked and fine

- XML parsing (`n1mm.rs`): quick-xml doesn't expand DTD entities, mismatched end tags are
  errors, and nothing panics on malformed input.
- `base_callsign` matches the SQL table. Band edges, freshness limits and `external_id` hashing
  match the contract's examples.
- `submit` rejects a result count that doesn't match the items, so the `zip` at
  `engine.rs:831` can't drop items silently.
- The token is never logged. `Token` and `DeviceCode` redact `Debug`/`Display`, and reqwest
  strips `Authorization` on a cross-host redirect.
- `browser_url_ok` refuses non-https URLs (http is allowed only for localhost), `user@host` and
  whitespace. The webview's capabilities are listen/unlisten only, with a strict CSP.
- Editing a contact logged more than 30 minutes ago in N1MM removes its spot without replacing
  it. That looks like a bug, but contract 6.3 says so explicitly.
- Sliding 120-per-minute rate window: stricter than the server's fixed windows, so it's safe.
