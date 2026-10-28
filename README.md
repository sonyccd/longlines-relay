# Long Lines Relay

Long Lines Relay is a small desktop app that sends the contacts you log to
[Long Lines](https://longlines.io) as spots. Your logging software already broadcasts each
contact on your computer in the N1MM Logger+ format; the relay catches those packets and posts
each contact, so your side of a contact sits next to what POTA, the Reverse Beacon Network and
the cluster heard. For example: "KK4PWJ worked W1AW on 14.025 CW at 21:14Z."

It runs on Windows 10 and 11 (x64), macOS (Apple Silicon and Intel) and Linux (x64).

**Privacy.** Logged contacts become spots that other Long Lines users can see and route, like
spots from any other source. The relay only listens for packets from this computer, keeps its
sign-in token in your operating system's credential store, and never writes the token to its
settings or log files.

- [Install](#install)
- [Set up your logging software](docs/loggers.md)
- [Development](#development)
- [Releasing](#releasing)
- [Updater key](#updater-key)
- [Testing an update](#testing-an-update)

## Install

Download the file for your computer from the
[latest release](https://github.com/sonyccd/longlines-relay/releases/latest). The relay is not
signed with an Apple or Microsoft developer certificate, so each operating system warns you the
first time you open it. The steps below get past that warning once; updates install without it.

| Computer | File |
| --- | --- |
| Windows 10 or 11 | `Long.Lines.Relay_<version>_x64-setup.exe` |
| Mac with Apple silicon (M1 or later) | `Long.Lines.Relay_<version>_aarch64.dmg` |
| Mac with an Intel processor | `Long.Lines.Relay_<version>_x64.dmg` |
| Linux, most distributions | `Long.Lines.Relay_<version>_amd64.AppImage` |
| Debian, Ubuntu and derivatives | `Long.Lines.Relay_<version>_amd64.deb` |

### Windows

1. Run the `-setup.exe` file. It installs for your user account only and doesn't need an
   administrator.
2. If Windows shows "Windows protected your PC" (Microsoft Defender SmartScreen), select
   **More info**, then **Run anyway**.

### macOS

1. Open the `.dmg` and drag **Long Lines Relay** to **Applications**.
2. Open it from Applications. macOS says it can't check the app for malicious software. To
   open it anyway:
   - **macOS 14 (Sonoma) and earlier:** Control-click (or right-click) the app in
     Applications, choose **Open**, then **Open** again.
   - **macOS 15 (Sequoia) and later:** select **Done**, open **System Settings > Privacy &
     Security**, scroll down to the message about Long Lines Relay, select **Open Anyway** and
     confirm with your password.
   - **Or, in Terminal:**

     ```sh
     xattr -dr com.apple.quarantine "/Applications/Long Lines Relay.app"
     ```

**Apple silicon.** Every app on an Apple silicon Mac must carry a signature. The relay is
signed ad hoc (no developer certificate), which is what lets it run once you have allowed it as
above. Without that signature macOS reports the app as "damaged" and offers only to move it to
the Trash; if you ever see that, download the release again rather than an older build.

### Linux: AppImage

```sh
chmod +x Long.Lines.Relay_*_amd64.AppImage
./Long.Lines.Relay_*_amd64.AppImage
```

AppImages need FUSE 2. On Ubuntu 22.04 install `libfuse2`; on Ubuntu 24.04 and later,
`libfuse2t64`.

### Linux: .deb

```sh
sudo apt install ./Long.Lines.Relay_*_amd64.deb
```

A `.deb` install can't update itself: when a new version is out, the relay shows a **Download**
link to the release instead of installing it. Install the new `.deb` the same way.

### Linux notes

- The relay keeps its sign-in token in the Secret Service (GNOME Keyring or KWallet). Most
  desktops run one; without one, signing in fails.
- The tray icon needs an AppIndicator-capable panel. On GNOME, that is the "AppIndicator and
  KStatusNotifierItem Support" extension (Ubuntu ships it on).

### Using the relay

1. Select **Sign in with Long Lines**. Your browser opens Long Lines; check that it shows the
   same code as the relay, then select **Allow**.
2. [Set up your logging software](docs/loggers.md) to send contacts to this computer on port
   12060.
3. Log contacts as usual. Each one shows up under "Contacts seen" and on Long Lines a moment
   later. Test mode shows contacts without sending them.

Closing the window keeps the relay running in the tray (menu bar on macOS); **Quit** is in the
tray menu, next to **Open log folder**. The relay starts when you log in unless you turn off
"Start when I log in" in its settings. You can disconnect a computer any time from **API
tokens** on Long Lines.

## Development

The repository is a Cargo workspace plus a Vite app:

| Path | What |
| --- | --- |
| `crates/relay-core` | Everything the relay does: the UDP listener, the N1MM parser, the checks, the outgoing queue, pairing and the Long Lines client. No Tauri dependency, so it builds and tests anywhere. |
| `src-tauri` | The Tauri 2 shell: window, tray, commands, updater, autostart. |
| `src` | The window (React, TypeScript, MUI). |
| `docs/contract.md` | The v1 API between the relay and Long Lines. |
| `docs/loggers.md` | Logger setup and the verified logger facts. It is the page "Set up logging software" opens. |

You need Rust (stable) and Node.js 22 or later. Building the shell on Linux also needs the
Tauri system libraries:

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libxdo-dev patchelf
```

### Tests

```sh
cargo test                     # the core (the workspace's default member); no system libraries needed
cargo clippy -p relay-core --all-targets -- -D warnings
cargo fmt --all -- --check
npm ci
npm test                       # the window (Vitest)
npm run lint                   # oxlint
npm run build                  # tsc + vite build
```

`crates/relay-core/tests/ui_shape.rs` pins the JSON the window reads to
`src/test/fixtures/snapshot.json`, which the window's tests load. After changing that shape on
purpose, rewrite the fixture with `UPDATE_UI_FIXTURE=1 cargo test -p relay-core --test ui_shape`
and fix the page. `crates/relay-core/tests/versions.rs` checks that every file carrying the
app's version agrees.

### fake-logger

`fake-logger` sends N1MM-format packets to the relay on this computer, so you can try the relay
without a logger:

```sh
cargo run -p relay-core --bin fake-logger -- --sequence info,replace,delete
cargo run -p relay-core --bin fake-logger -- --mycall K4XYZ          # a callsign mismatch
cargo run -p relay-core --bin fake-logger -- --offset-minutes 45     # too old to send
cargo run -p relay-core --bin fake-logger -- --malformed             # "Couldn't read packet"
cargo run -p relay-core --bin fake-logger -- --help
```

### Running against a local Long Lines

1. In the Long Lines repository, start the local stack and serve the functions:

   ```sh
   supabase start
   supabase functions serve
   ```

2. Here, run the relay against it:

   ```sh
   LONGLINES_RELAY_API_BASE=http://127.0.0.1:54321/functions/v1/relay-api/v1 npm run tauri dev
   ```

   `LONGLINES_RELAY_API_BASE` replaces the production base URL
   (`https://api.longlines.io/relay/v1`). Use the API port `supabase start` printed if it isn't
   54321.

3. Sign in. The approval page opens on the local web app (`cd web && npm run dev` in the Long
   Lines repository), at the address the local server returns.

Development builds never check for updates and never register a login item. Logs go to the
terminal and to the log folder (tray menu, **Open log folder**).

## Releasing

1. Bump the version in all four places, in one commit:
   - `Cargo.toml` (`[workspace.package] version`), then run `cargo check` so `Cargo.lock`
     follows;
   - `src-tauri/tauri.conf.json` (`version`), which names the bundles and is what the updater
     compares;
   - `package.json` and `package-lock.json`: `npm version 0.1.1 --no-git-tag-version`.

   `cargo test` fails if they disagree.
2. Merge to main.
3. Tag the merge and push the tag:

   ```sh
   git tag v0.1.1
   git push origin v0.1.1
   ```

4. The Release workflow checks that the tag matches the version, creates a draft release, builds
   Windows, macOS (Apple silicon and Intel) and Linux, uploads the bundles, their updater
   signatures and `latest.json`, then checks that `latest.json` has a signed entry for every
   platform. If a build fails, re-run the failed jobs; they upload to the same draft.
5. Download a build or two from the draft and try them.
6. Publish the draft (`gh release edit v0.1.1 --draft=false`, or **Publish release** on
   GitHub). Installed relays read
   `https://github.com/sonyccd/longlines-relay/releases/latest/download/latest.json`, which
   only a published release answers, so nothing reaches users before this step. They see the
   update within a day, or at their next launch.

## Updater key

Updates are signed with the relay's own key pair, separate from any OS code signing.

- The pair was generated on Brad's machine with `npx tauri signer generate -w
  ~/.tauri/longlines-relay.key`, with an empty password. The private key is
  `~/.tauri/longlines-relay.key`; the public key is `~/.tauri/longlines-relay.key.pub`, and its
  contents are `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.
- The Release workflow reads the private key from the repository secret
  `TAURI_SIGNING_PRIVATE_KEY`. Set it once:

  ```sh
  gh secret set TAURI_SIGNING_PRIVATE_KEY --repo sonyccd/longlines-relay < ~/.tauri/longlines-relay.key
  ```

  The key has no password, so leave `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` unset. If you ever
  generate a key with a password, set that secret too.
- Pull request builds don't get the secret: CI builds with `src-tauri/tauri.ci.conf.json`, which
  turns the updater artifacts off.
- Keep a copy of the private key somewhere safe outside this computer (a password manager). The
  `.gitignore` keeps `*.key` files out of the repository; never commit it.
- **Losing the private key breaks updates.** Every installed relay only accepts updates signed
  with it, so without it no installed copy can update itself: users would have to download and
  install a new version by hand. To change keys on purpose, ship a release signed with the old
  key whose `tauri.conf.json` carries the new public key, then sign later releases with the new
  key.

## Testing an update

Do this once, with v0.1.0 and v0.1.1, to prove the update path on every platform, and again
after changing anything in the updater, the bundle settings or the Release workflow.

1. Release and publish v0.1.0 as above.
2. Install v0.1.0 on each of: Windows (`-setup.exe`), a Mac with Apple silicon and an Intel Mac
   (`.dmg`), Linux from the AppImage, and Linux from the `.deb`. Sign in on each and log a
   contact with `fake-logger` so there is a token and a queue to keep.
3. Bump to 0.1.1, then release and publish v0.1.1.
4. On each computer, quit the relay and open it again (it checks 10 seconds after launch, then
   daily). Expect:
   - **Windows, macOS, AppImage:** a banner "Version 0.1.1 of the relay is available." Select
     **Install and restart**. The relay restarts on its own, the Settings card shows "Version
     0.1.1", and it is still signed in.
   - **.deb:** the banner offers **Download**, which opens the v0.1.1 release page. Install the
     new `.deb` with apt, quit the relay from the tray and open it again; the Settings card
     shows "Version 0.1.1" and it is still signed in.
5. Check the log folder for `update installed; restarting`, and that no line holds the token.
6. If a computer shows no banner, look in its log for `couldn't check for updates` and compare
   the release's `latest.json` with the platform names in the Release workflow's check.
