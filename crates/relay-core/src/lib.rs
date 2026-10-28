//! Long Lines Relay core.
//!
//! Everything the relay does lives here, with no Tauri dependency, so `cargo test` covers it on
//! any machine.

pub mod bands;
pub mod callsign;
pub mod external_id;
pub mod freshness;
pub mod status;

/// The core crate's version. The app passes its own version to the client; this is the default.
pub const RELAY_VERSION: &str = env!("CARGO_PKG_VERSION");
