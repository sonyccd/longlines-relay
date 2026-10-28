//! Self-update from GitHub Releases: a check at launch and daily; the page offers the update
//! and installs it on request, then the app restarts.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{emit_state, Relay};

/// The first check waits for the app to settle.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(10);
/// Then once a day.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The update banner's state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateView {
    /// Up to date, or not checked yet.
    #[default]
    None,
    Available {
        version: String,
    },
    Installing {
        version: String,
    },
    Failed {
        version: String,
    },
}

#[derive(Default)]
pub struct Updates {
    view: Mutex<UpdateView>,
    pending: tokio::sync::Mutex<Option<Update>>,
}

impl Updates {
    pub fn view(&self) -> UpdateView {
        self.view.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_view(&self, view: UpdateView) {
        *self.view.lock().unwrap_or_else(|e| e.into_inner()) = view;
    }
}

/// Checks at launch and daily. Development builds never check.
pub fn spawn_checks(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_AFTER).await;
        loop {
            check(&app).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

async fn check(app: &AppHandle) {
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(e) => {
            tracing::warn!(error = %e, "the updater isn't available");
            return;
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            tracing::info!(version = %update.version, "an update is available");
            let relay = app.state::<Relay>();
            let mut pending = relay.updates.pending.lock().await;
            if !matches!(relay.updates.view(), UpdateView::Installing { .. }) {
                relay.updates.set_view(UpdateView::Available {
                    version: update.version.clone(),
                });
                *pending = Some(update);
            }
            drop(pending);
            emit_state(app);
        }
        Ok(None) => tracing::info!("the relay is up to date"),
        Err(e) => tracing::warn!(error = %e, "couldn't check for updates"),
    }
}

/// Downloads and installs the offered update, then restarts. On Windows the installer closes
/// the app itself.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let relay = app.state::<Relay>();
    let mut pending = relay.updates.pending.lock().await;
    let Some(update) = pending.take() else {
        return Err("No update is waiting.".into());
    };
    let version = update.version.clone();
    relay.updates.set_view(UpdateView::Installing {
        version: version.clone(),
    });
    emit_state(app);
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => {
            tracing::info!(%version, "update installed; restarting");
            app.restart();
        }
        Err(e) => {
            tracing::error!(error = %e, %version, "couldn't install the update");
            *pending = Some(update);
            relay.updates.set_view(UpdateView::Failed { version });
            drop(pending);
            emit_state(app);
            Err(e.to_string())
        }
    }
}
