//! Self-update from GitHub Releases: a check at launch and daily; the page offers the update
//! and installs it on request, then the app restarts.
//!
//! A copy installed from a .deb (or .rpm) package is owned by the system package manager, so
//! it never replaces itself: the page links to the release instead (`UpdateView::Manual`).

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::utils::config::BundleType;
use tauri::utils::platform::bundle_type;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{emit_state, open_in_browser, Relay};

/// The repository's releases. A release's page is `<this>/tag/v<version>`.
pub const RELEASES_URL: &str = "https://github.com/sonyccd/longlines-relay/releases";

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
    /// An update this copy can't install over itself (a .deb or .rpm install): the page links
    /// to the release so the user downloads the new package.
    Manual {
        version: String,
        url: String,
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

/// Whether this copy can install an update over itself. On Linux only an AppImage can: a .deb
/// or .rpm belongs to the package manager, and a bare binary (no bundle) has nothing to
/// replace. Windows (NSIS) and macOS (.app) always can.
pub fn installs_in_place(os: &str, bundle: Option<BundleType>) -> bool {
    os != "linux" || bundle == Some(BundleType::AppImage)
}

/// The release page for `version`.
pub fn release_page(version: &str) -> String {
    format!("{RELEASES_URL}/tag/v{version}")
}

/// What the page shows once `version` is found.
fn offered(version: &str, in_place: bool) -> UpdateView {
    if in_place {
        UpdateView::Available {
            version: version.to_owned(),
        }
    } else {
        UpdateView::Manual {
            version: version.to_owned(),
            url: release_page(version),
        }
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
                let in_place = installs_in_place(std::env::consts::OS, bundle_type());
                relay.updates.set_view(offered(&update.version, in_place));
                // A manual update is never installed from here.
                *pending = in_place.then_some(update);
            }
            drop(pending);
            emit_state(app);
        }
        Ok(None) => tracing::info!("the relay is up to date"),
        Err(e) => tracing::warn!(error = %e, "couldn't check for updates"),
    }
}

/// Opens the release page of a manual update. The URL comes from the shell's own view, never
/// from the page.
pub fn open_release_page(app: &AppHandle) -> Result<(), String> {
    match app.state::<Relay>().updates.view() {
        UpdateView::Manual { url, .. } => {
            open_in_browser(app, &url);
            Ok(())
        }
        _ => Err("No update is waiting.".into()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_appimage_updates_itself_on_linux() {
        assert!(installs_in_place("linux", Some(BundleType::AppImage)));
        assert!(!installs_in_place("linux", Some(BundleType::Deb)));
        assert!(!installs_in_place("linux", Some(BundleType::Rpm)));
        assert!(!installs_in_place("linux", None));
    }

    #[test]
    fn windows_and_macos_update_themselves() {
        assert!(installs_in_place("windows", Some(BundleType::Nsis)));
        assert!(installs_in_place("windows", None));
        assert!(installs_in_place("macos", Some(BundleType::App)));
    }

    #[test]
    fn a_deb_install_gets_the_release_page() {
        assert_eq!(
            offered("0.1.1", false),
            UpdateView::Manual {
                version: "0.1.1".into(),
                url: "https://github.com/sonyccd/longlines-relay/releases/tag/v0.1.1".into(),
            }
        );
        assert_eq!(
            offered("0.1.1", true),
            UpdateView::Available {
                version: "0.1.1".into()
            }
        );
    }

    #[test]
    fn the_view_serializes_as_the_page_reads_it() {
        let json = serde_json::to_value(offered("0.1.1", false)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "state": "manual",
                "version": "0.1.1",
                "url": "https://github.com/sonyccd/longlines-relay/releases/tag/v0.1.1",
            })
        );
    }
}
