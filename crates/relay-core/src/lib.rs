//! Long Lines Relay core.
//!
//! Everything the relay does lives here, with no Tauri dependency, so `cargo test` covers it on
//! any machine. The Tauri shell (`src-tauri`) only wires commands, the tray and the window to
//! [`engine::Engine`].
//!
//! Data flow: a [`listeners::Listener`] receives datagrams, the engine's pipeline parses them
//! ([`n1mm`]), applies the callsign ([`callsign`]) and freshness ([`freshness`]) checks, puts
//! contacts in the on-disk [`upstream::queue`], and the sender loop submits them with the
//! [`upstream::client`]. Every step publishes to the [`bus`]; [`state`] keeps what the UI shows.

pub mod bands;
pub mod bus;
pub mod callsign;
pub mod engine;
pub mod external_id;
pub mod fake_logger;
pub mod freshness;
pub mod listeners;
pub mod logging;
pub mod n1mm;
pub mod settings;
pub mod source_filter;
pub mod state;
pub mod status;
pub mod upstream;

/// The core crate's version. The app passes its own version to the client; this is the default.
pub const RELAY_VERSION: &str = env!("CARGO_PKG_VERSION");
