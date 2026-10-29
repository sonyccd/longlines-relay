# Security review: Long Lines Relay

Full-repository audit of the code on disk at `de08662` (branch `review-fixes`), 2026-10-29.

**Status:**

- **Fixed on the `security-fixes` branch:** L1, L2, L4, L5, L6, and GitHub code scanning
  alert #1.
- **Partly fixed:** M1 and L3. Their workflow side is done; the repository settings steps they
  need are listed under each one.
- **Open:** Dependabot alert #1 (glib).

Each finding ends with what changed. Line references are as of `de08662`.

## Summary

| Severity | Count |
| --- | --- |
| Critical | 0 |
| High | 0 |
| Medium | 1 |
| Low | 6 |

The application code holds up well. Its local-only listeners, the sanitised output of the DX
cluster, parameterised SQL, redacted tokens, rustls with no TLS overrides, a strict CSP and a
minimal Tauri capability leave little room for injection or token leakage. The most significant
risk is in the release pipeline: the updater signing key, which has no password, is exposed to
the whole build toolchain. If that key is stolen, every installed relay will auto-install
whatever an attacker signs with it. Every Low finding needs either a position on the local
network plus OS behaviour that is still untested, or an unusual failure or race.

- `npm audit` (full and `--omit=dev`): 0 vulnerabilities.
- `cargo audit` (v0.22.2, 1,386 advisories, 577 crates): 0 vulnerabilities and 5 informational
  warnings (see [Dependencies](#dependencies)).
- **False positives dropped: 56.** The area reviewers proposed these as candidates and then
  dropped them after tracing each source to its sink (see
  [Dropped candidates](#dropped-candidates)). All 7 findings below were traced again against the
  code before going into this report. None of them was dropped at that stage.

## Codebase map

| Area | Files | Role |
| --- | --- | --- |
| Input: UDP | `crates/relay-core/src/listeners/udp.rs`, `source_filter.rs`, `n1mm.rs` | Binds `0.0.0.0:<port>` (default 12060), drops packets whose source isn't loopback or one of this computer's own addresses, parses N1MM XML with quick-xml |
| Input: DX cluster | `crates/relay-core/src/cluster/{server,line,mod}.rs` | Optional telnet server. Off by default; binds loopback, or `0.0.0.0` when the user turns on LAN access. No password |
| Upstream | `crates/relay-core/src/upstream/{client,auth,token_store,queue}.rs` | reqwest with rustls to longlines.io, RFC 8628 device pairing, bearer token in the OS credential store (keyring), SQLite outbox |
| Downstream | `crates/relay-core/src/downstream/{sse,stream}.rs` | SSE spots stream from the URL that `GET /me` returns (https only, or http to loopback) |
| Orchestration | `crates/relay-core/src/{engine,state,bus,status}.rs` | Pipeline, sender loop, sign-in and sign-out, UI snapshot |
| Shell | `src-tauri/src/{lib,commands,updates,view,tray}.rs`, `tauri.conf.json`, `capabilities/default.json` | Tauri 2: IPC commands, updater (minisign, GitHub Releases), autostart, single instance |
| Webview | `src/**` | React/MUI page. Receives state events and calls commands that take only `u16`/`bool` arguments |
| CI/CD | `.github/workflows/{ci,release}.yml` | PR checks; a tag push builds, signs and publishes releases and `latest.json` |

**Trust boundaries.** The untrusted inputs are:

- UDP datagrams, which only local programs should be able to send;
- connections from cluster clients;
- server data received over TLS, which is trusted as the issuer of the token;
- the build-time supply chain: npm packages, crates and GitHub Actions.

---

## Medium

### M1. The updater signing key, which has no password, is exposed to the whole build toolchain alongside a write token

- **Where:** `.github/workflows/release.yml:119-125`, `README.md:267-280`
- **Confidence:** High
- **What's wrong:**
  - The `tauri-action` step gets `TAURI_SIGNING_PRIVATE_KEY` and `GITHUB_TOKEN` (`contents: write`)
    as step environment variables.
  - That step runs `tauri build`, which runs `npm run build` (tsc, vite and their plugins) and
    `cargo build`.
  - `cargo build` runs every `build.rs` and proc macro across about 575 locked crates.
  - Any of that code can read both values. The README states the key was generated with an
    empty password, so the secret value alone is enough to sign updates.
- **Exploit scenario:**
  1. A compromised version of a build-time npm package or crate lands in a lockfile through a
     routine dependency bump.
  2. On the next tag build, it reads the key and the token.
  3. With the token it can replace release assets or `latest.json` during the job.
  4. With the key it can sign a malicious update at any later time.
  5. Installed relays verify the signature and auto-install the update. Windows uses
     `installMode: passive`.
  6. Rotating the key requires shipping a release signed with the old key, and the attacker can
     sign one too.
- **Fix (full):**
  - Build without the key (`contents: read`) and upload the unsigned bundles as artifacts.
  - Sign in a separate minimal job that runs no `npm ci` and no cargo build. It runs only a
    pinned `tauri signer sign`/minisign over the artifacts and reads the key from a GitHub
    Environment with required reviewers.
  - Upload the assets and `latest.json` from a third job that has `contents: write`.
  - Also give the key a password, stored as a separate Environment secret.
- **Partly fixed:**
  - `npm run build` now runs in its own step before the signing step, so Vite, tsc and their
    plugins never see the key. `src-tauri/tauri.release.conf.json` drops `beforeBuildCommand`,
    so `tauri build` doesn't run it again.
  - The build job runs in the `release` environment, and Cargo builds with `--locked` (L4).
  - Still open: Cargo build scripts and proc macros run in the signing step, and anything run
    earlier in the same job could tamper with it. Only the full job split closes that.
  - Repository side: create the `release` environment with a required reviewer and a `v*`
    tag rule, and move `TAURI_SIGNING_PRIVATE_KEY` into it (README "Updater key"). Delete the
    repository secret, and consider rotating to a key with a password.

---

## Low

### L1. A host on the LAN may be able to spoof this computer's own address to get past the UDP "local only" filter, then post forged contacts or delete real ones

- **Where:** `crates/relay-core/src/source_filter.rs:62-77`, `listeners/udp.rs:70`,
  `listeners/udp.rs:138-141`, `engine.rs:1070` (`decide_delete`)
- **Confidence:**
  - High that the code behaves as described.
  - Low that it can be exploited on Windows and macOS (untested; see Gaps).
  - Not exploitable on Linux with default sysctls (`accept_local=0` drops such packets).
- **What's wrong:**
  - The source-address check is the only access control on the contacts pipeline.
  - It allows loopback sources and any address on one of this computer's own interfaces. It has
    to, because loggers that send to the LAN IP or the broadcast address arrive with that source
    (`docs/loggers.md`).
  - UDP source addresses can be forged. A packet arriving from the network with
    `src = <victim's LAN IP>` passes the filter whenever the OS delivers it.
  - Upserts must match the account's callsign (`engine.rs:1028`, `1157`), but callsigns are
    public.
  - Deletes have no callsign check at all. The fallback `external_id` is a SHA-256 of
    `mycall|call|band|mode|timestamp`, all of which can be seen in a published spot.
- **Exploit scenario:**
  1. On shared Field Day or club Wi-Fi, an attacker sends `<contactinfo>` to UDP 12060 with a
     spoofed source equal to the victim's IP, the victim's callsign as `mycall`, and a current
     timestamp.
  2. The relay posts a fabricated spot under the victim's account.
  3. `<contactdelete>` packets built from the victim's published spots remove their real spots.
  4. Requirements: the same network segment, a host firewall that allows inbound UDP to the app,
     and an OS that accepts arriving packets with its own address as the source.
- **Fix:**
  - Prefer a loopback-only mode as the default, with loggers sending to `127.0.0.1`. Make
    accepting own-interface sources an opt-in for loggers that broadcast.
  - Or use `IP_PKTINFO`/`IP_RECVIF` to require that unicast packets arrive on the loopback
    interface, and allow other interfaces only for broadcast destinations.
  - Apply the `same_station` check to deletes as well, when `mycall` is present.
- **Fixed:**
  - `SourceFilter` now accepts loopback sources only. This computer's own interface addresses
    are no longer trusted, since anyone on the network can put them on a forged packet.
  - Loggers already send to `127.0.0.1` (`docs/loggers.md`). MacLoggerDX's default broadcast
    is now ignored, and the docs say its `broadcast_ip_address` setting is required.
  - The callsign check on deletes was not added. N1MM's `contactdelete` has no `mycall`, and an
    attacker picks `mycall` anyway, so it would block nothing.

### L2. The release workflow grants `contents: write` to every job, and checkout leaves the token in git config

- **Where:** `.github/workflows/release.yml:19-20` (top-level `permissions: contents: write`),
  `release.yml:36,83` and `ci.yml:28,47,80` (`actions/checkout` without
  `persist-credentials: false`)
- **Confidence:** High
- **What's wrong:**
  - The `check` job only reads but receives write access.
  - In `build`, checkout writes the write-scoped token to `.git/config` before `npm ci`
    (`release.yml:97`). Any package install script, and anything it plants, can read it.
- **Exploit scenario:** a malicious install script reads the token from git config. While the
  job runs, it tampers with release assets or pushes to unprotected refs.
- **Fix:**
  - Set top-level `permissions: {}` and grant `contents: write` only to `draft` and `build`.
  - Give `check` `contents: read`.
  - Add `persist-credentials: false` to every checkout.
- **Fixed:**
  - Top-level `permissions: {}`. `draft`, `build` and `check` each ask for `contents: write`:
    `check` reads the draft release, and only accounts that can push may see a draft.
  - `persist-credentials: false` on every checkout in both workflows.

### L3. The signing key is a repository secret with no Environment gate, and any `v*` tag on any commit produces a signed release

- **Where:** `.github/workflows/release.yml:15-17` (trigger), `release.yml:124-125` (secrets).
  No `environment:` is set.
- **Confidence:** Medium. Repository settings such as rulesets and collaborators can't be seen
  from disk.
- **What's wrong:** any account with write access, or a stolen maintainer token, can get a signed
  update out in either of two ways:
  - push a branch with a workflow that echoes `secrets.TAURI_SIGNING_PRIVATE_KEY`;
  - push a `v*` tag on an unreviewed commit.

  Nothing checks that the tagged commit is on `main`. The `git tag vX origin/main` step in the
  README is only a convention.
- **Exploit scenario:** a stolen maintainer token pushes `v9.9.9` on a commit with a backdoor.
  The workflow signs it and publishes `latest.json`, and every relay updates.
- **Fix:**
  - Move both signing secrets into an Environment `release` that is limited to `v*` tags and has
    a required reviewer.
  - Add a tag ruleset that restricts who can create `v*` tags.
  - Optionally fail `draft` unless `git merge-base --is-ancestor "$GITHUB_SHA" origin/main`
    succeeds.
- **Partly fixed:**
  - `draft` now fails unless the tagged commit is on `main`.
  - The build job uses the `release` environment.
  - Repository side: the environment's protection rules, moving the secret, and a tag ruleset
    for `v*` (see M1).

### L4. Cargo builds in CI and release don't use `--locked`

- **Where:** `.github/workflows/release.yml:135` (`args: ${{ matrix.args }}`), `ci.yml:37,39,41,109,112`
- **Confidence:** High
- **What's wrong:**
  - If a manifest change is merged without its `Cargo.lock` update, CI quietly resolves new crate
    versions instead of failing.
  - The release build does the same next to the signing key (M1).
  - The version-stamp step rewrites `Cargo.toml` before the build, which already means the
    lockfile is regenerated during the build.
  - npm is not affected, because `npm ci` is strict about the lockfile.
- **Exploit scenario:** a dependency drift that no one reviewed pulls a freshly published,
  malicious crate version into a release build.
- **Fix:**
  - After stamping, run `cargo update --workspace --offline`, then build with
    `-- --locked`.
  - Add `--locked` to the CI `cargo clippy`/`cargo test` calls so drift fails the PR.
- **Fixed:**
  - The version stamp also writes the tag's version into the two workspace crates' entries
    in `Cargo.lock`.
  - Release and CI bundles build with `-- --locked`.
  - Every CI `cargo clippy`/`cargo test` passes `--locked`.

### L5. A `GET /me` that is in flight during sign-out restores the account, and the sender then posts one batch with the signed-out token

- **Where:** `crates/relay-core/src/engine.rs:1447` (token loaded), `1466` (awaits `me`),
  `800-823` (`account_ok` restores the account without checking the token is still current),
  `1500`, `1521` (`send_batch`). Related: `engine.rs:712-751`, where `sign_in` never clears
  `state.account`.
- **Confidence:** Medium. It is a real race, but the window is the `/me` round trip, up to the
  30 s timeout.
- **What's wrong:**
  - `sign_out` → `signed_out` clears the account (`engine.rs:847-848`).
  - A `/me` that was already in flight then calls `account_ok`, which puts the account back.
  - The sender sees an account and submits the next batch with the token it loaded before
    sign-out. This also happens when the delete failed, which breaks the "never used" promise
    in the comment at `engine.rs:1440`.
  - The restored stale account can also linger into a later sign-in as a different account
    whose first `/me` fails. That account's contacts are then checked against the old callsign.
    The server's `mycall_mismatch` check prevents cross-account posting, so the client-side
    effect is only that contacts are wrongly dropped.
- **Exploit scenario:** no attacker drives this. A user signs out during the hourly account
  refresh, for example to hand the machine over, and one more batch of their own contacts is
  sent with the old token.
- **Fix:**
  - Add an auth generation counter that `sign_in`, `sign_out` and `signed_out` bump.
  - Drop the result of `account_ok`/`send_batch` when the generation changed, or when
    `tokens.load()` no longer returns the same token.
  - Call `state.set_account(None)` at the start of `sign_in`.
- **Fixed:**
  - `Shared::token_current` checks that the token is still the stored one and that no
    sign-out is pending.
  - The sender checks it after `GET /me` returns. On a stale answer it drops the answer and
    sends nothing.
  - `sign_in` checks it before `account_ok`, and now clears the previous account first.
  - Test: `an_account_answer_that_arrives_after_sign_out_is_ignored` (`tests/resilience.rs`).

### L6. If both the credential-store delete and the marker write fail, the next launch signs back in with the token the user signed out of

- **Where:** `crates/relay-core/src/engine.rs:556-563` (`sign_out`), `754-766` (marker write
  failure is only logged), `287-296` (startup trusts the absence of the marker)
- **Confidence:** Medium. The code path is real, but it needs two independent failures at once.
- **What's wrong:**
  - When `tokens.delete()` fails, the `sign-out-pending` marker is all that stops the old token
    from being used after a restart.
  - If writing the marker also fails (read-only or full disk, a permissions problem), the code
    logs a warning and never tries the write again.
  - On the next launch, `load()` returns the old token and the relay reconnects, posts contacts
    and opens the spots stream.
  - The token stays valid on the server, because the contract has no revoke route.
- **Exploit scenario:** the keyring is locked or broken and the data directory can't be
  written. The user signs out before handing the computer to someone else. After a restart the
  relay is signed in to their account again.
- **Fix:**
  - Retry the marker write on each sender tick while `sign_out_incomplete` is true.
  - If neither the delete nor the marker can be persisted, overwrite the keyring entry with a
    tombstone value that `load` treats as no token.
  - Tell the user that sign-out could not be made to stick.
- **Fixed:**
  - While a sign-out is incomplete, the sender's retry now also rewrites a missing marker.
  - Test: `a_sign_out_marker_that_couldnt_be_written_is_written_later`.
  - Not done: the tombstone value. When the delete fails, a save to the same store is likely
    to fail too.

---

## Dependencies

`cargo audit` reported 0 vulnerabilities. Its informational warnings:

| Advisory | Crate | Kind | Reachability |
| --- | --- | --- | --- |
| RUSTSEC-2024-0370 | proc-macro-error 1.0.4 | Unmaintained | Build time only (gtk3 stack used by the Linux tray) |
| RUSTSEC-2026-0370/0371/0372 | zvariant 5.15.1, zbus_names 4.3.5, zvariant_derive 5.15.1 | Unmaintained | Runtime on Linux through keyring (secret-service), the opener and single-instance. Only peers on the same user's session bus can reach them. No known vulnerability; watch for updates to zbus and keyring |
| RUSTSEC-2024-0429 | glib 0.18.5 | Unsound (`VariantStrIter`) | Linux tray stack. The app never uses glib directly. Fixing it needs Tauri to move to glib ≥0.20 |

Other checks:

- Every entry in `Cargo.lock` comes from crates.io, with no git sources.
- Every npm entry resolves from registry.npmjs.org with an integrity hash.
- There are no npm lifecycle scripts. The only package with an install script is fsevents (dev,
  macOS only).
- All 5 GitHub Actions are pinned to commit SHAs, and each SHA matches the tag named in its
  comment.

## GitHub scanning

- **Code scanning alert #1** (CodeQL `rust/cleartext-logging`,
  `crates/relay-core/tests/e2e.rs:101`). The ignored end-to-end test printed the account's
  callsign and email. It is a test file, but there was no reason to print them.
  **Fixed:** the `println!` is removed.
- **Dependabot alert #1** (glib 0.18.5, GHSA-wrw7-89jp-8q8g / RUSTSEC-2024-0429). This is the
  same advisory as the table above.
  - It is unfixable here: it comes in through Tauri's Linux gtk3 and tray stack, and only a
    Tauri release on glib 0.20 or later removes it.
  - The app never calls `VariantStrIter`.
  - **Left open**, to be closed by a Tauri upgrade.

## Dropped candidates

Area reviewers dropped 56 candidates after tracing them. The notable ones:

- **Network input (14 dropped):**
  - XXE is not possible: quick-xml expands no DTD entities.
  - IPv4-mapped IPv6 can't bypass the filter: the socket is IPv4-only and both sides are
    canonicalised.
  - Spoofed 127/8 sources are dropped as martian by every OS before they reach the filter.
  - CR/LF or telnet IAC injection into cluster output is blocked: fields are limited to
    printable ASCII and the time is restricted to four digits (`cluster/line.rs:37-89`).
  - Cluster clients can only send bye/quit/dx/prompt and cannot reach the engine.
- **Upstream (8):**
  - The token can't cross hosts or protocols: reqwest strips `Authorization` on a cross-origin
    redirect, and the stream client uses `Policy::none()`.
  - `browser_url_ok` and `stream_url_ok` hold up against bypass attempts.
  - All SQL is parameterised.
  - The settings file can't change the API host.
  - The token's Debug and Display output is redacted, and reqwest errors are logged
    `without_url()`.
- **Engine (13):**
  - No secrets reach the Snapshot or bus events: `DeviceCode` and the token are never
    serialised, and `external_id` is `serde(skip)`.
  - The marker path is fixed.
  - UDP and cluster input can't trigger privileged actions.
- **Shell and frontend (8):**
  - There is no `dangerouslySetInnerHTML`, `innerHTML`, data-derived `href` or `window.open`.
  - IPC commands accept only `u16` and `bool`.
  - The capability grants only event listen and unlisten.
  - The CSP allows no remote origins.
  - The updater is signed and served over HTTPS.
  - The single-instance argv and cwd are ignored.
  - Autostart passes a fixed `--hidden` argument.
- **CI and secrets (13):**
  - No `${{ github.event.* }}` appears in `run:` steps.
  - There is no `pull_request_target`, `workflow_run` or `issue_comment` trigger.
  - The tag name is checked against a regex before use.
  - There is no artifact or cache path from PRs into the release.
  - No secrets turned up in the working tree, the full history, or unreachable objects.
    The tokens in docs and tests are placeholders.

Hardening notes that were dropped as findings but are worth knowing:

- `LONGLINES_RELAY_API_BASE` is honoured in release builds and accepts `http://` to any host
  (`upstream/client.rs:66-72`, `src-tauri/src/lib.rs:202`). Exploiting it requires control of
  the user's environment.
- The CSP keeps `style-src 'unsafe-inline'`, which MUI requires. There is no injection point to
  use it from.

## Gaps: areas not covered or not assessable

- **Windows and macOS packet handling (decides L1).** It is untested whether these stacks accept
  an arriving UDP packet whose source is one of the computer's own unicast addresses, and whether
  Windows Defender Firewall lets inbound UDP reach the app after install. A two-host test with
  scapy would settle it.
- **Server side.** These are out of scope (they are not in this repo):
  - how the pairing approval page binds a `user_code` to an account;
  - how tokens are generated and revoked;
  - whether deletes and upserts are scoped by account and callsign;
  - device-code phishing protections.
- **GitHub repository settings.** These can't be seen from disk: collaborators, branch and tag
  rulesets, existing Environments, whether `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is set, and
  the contents of releases already published.
- **Runtime behaviour.** Nothing was run.
  - Not checked: Tauri's ACL enforcement of the plugin commands that are registered but not
    granted.
  - Not checked: the CSP nonce injection.
  - Not checked: how the keyring backends fail in practice, which sets how likely L6 is.
- **Third-party code internals.** tauri-action, the updater plugin's Linux `.deb` install path
  (`pkexec dpkg -i` after verifying the signature) and reqwest's proxy handling were reviewed
  only at their boundaries.
- **Published CVEs in tauri and plugin versions** beyond what RustSec and npm advisories cover.
- **Tests and dev tooling.** Test files were not audited. The `fake-logger` dev binary was read
  only enough to confirm that it only sends to loopback.
