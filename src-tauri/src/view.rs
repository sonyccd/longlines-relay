//! Everything the page draws, in one serializable value.

use relay_core::state::{Snapshot, UpstreamStatus};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::updates::UpdateView;
use crate::Relay;

#[derive(Clone, Debug, Serialize)]
pub struct SettingsView {
    pub port: u16,
    pub autostart: bool,
}

/// The `state` event and `get_state`'s answer.
#[derive(Clone, Debug, Serialize)]
pub struct ShellView {
    /// `account`, `feed`, `upstream`, `pairing` and `stream`.
    #[serde(flatten)]
    pub snapshot: Snapshot,
    /// A token is stored: the main screen shows, even before `GET /me` answers.
    pub signed_in: bool,
    pub settings: SettingsView,
    pub update: UpdateView,
    pub version: String,
}

pub fn build(app: &AppHandle) -> ShellView {
    let relay = app.state::<Relay>();
    let snapshot = relay.engine.snapshot();
    let settings = relay.settings();
    ShellView {
        signed_in: snapshot.upstream != UpstreamStatus::SignedOut,
        snapshot,
        settings: SettingsView {
            port: settings.port,
            autostart: settings.autostart,
        },
        update: relay.updates.view(),
        version: app.package_info().version.to_string(),
    }
}
