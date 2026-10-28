//! The page's commands: a thin layer over the core engine. Every command is `async` so it runs
//! on Tauri's Tokio runtime, which the engine's listener needs.

use relay_core::upstream::auth::{browser_url_ok, device_name_from, PairingView};
use tauri::{AppHandle, State};

use crate::view::{self, ShellView};
use crate::{emit_state, open_in_browser, Relay, RELAY_SETUP_DOCS_URL, SIGN_UP_URL};

/// Opens the pairing page if the server's URL is one the browser may open.
fn open_pairing_page(app: &AppHandle, view: &PairingView) {
    if let PairingView::Waiting {
        verification_uri_complete,
        ..
    } = view
    {
        if browser_url_ok(verification_uri_complete) {
            open_in_browser(app, verification_uri_complete);
        } else {
            tracing::warn!("the pairing URL isn't one the relay opens");
        }
    }
}

#[tauri::command]
pub async fn get_state(app: AppHandle) -> ShellView {
    view::build(&app)
}

/// "Sign in with Long Lines": starts pairing and opens the approval page in the browser.
#[tauri::command]
pub async fn start_sign_in(app: AppHandle, relay: State<'_, Relay>) -> Result<ShellView, String> {
    let host = gethostname::gethostname();
    let device_name = device_name_from(&host.to_string_lossy());
    if let Ok(view) = relay.engine.start_sign_in(&device_name).await {
        open_pairing_page(&app, &view);
    }
    // A failure is in the state's `pairing`; the page shows it.
    Ok(view::build(&app))
}

/// "Open browser again".
#[tauri::command]
pub async fn open_sign_in_page(app: AppHandle, relay: State<'_, Relay>) -> Result<(), String> {
    open_pairing_page(&app, &relay.engine.snapshot().pairing);
    Ok(())
}

#[tauri::command]
pub async fn cancel_sign_in(app: AppHandle, relay: State<'_, Relay>) -> Result<ShellView, String> {
    relay.engine.cancel_sign_in();
    Ok(view::build(&app))
}

/// Deletes the token from the credential store.
#[tauri::command]
pub async fn sign_out(app: AppHandle, relay: State<'_, Relay>) -> Result<ShellView, String> {
    relay.engine.sign_out().map_err(|e| e.to_string())?;
    Ok(view::build(&app))
}

#[tauri::command]
pub async fn set_test_mode(
    app: AppHandle,
    relay: State<'_, Relay>,
    on: bool,
) -> Result<ShellView, String> {
    relay.engine.set_test_mode(on);
    Ok(view::build(&app))
}

/// Moves the listener to `port` and saves it. A port in use shows on the feed card; the relay
/// never picks another port by itself.
#[tauri::command]
pub async fn set_port(
    app: AppHandle,
    relay: State<'_, Relay>,
    port: u16,
) -> Result<ShellView, String> {
    if port == 0 {
        return Err("Port must be from 1 to 65535.".into());
    }
    relay.engine.set_port(port);
    relay.update_settings(|s| s.port = port)?;
    emit_state(&app);
    Ok(view::build(&app))
}

/// "Start when I log in".
#[tauri::command]
pub async fn set_autostart(
    app: AppHandle,
    relay: State<'_, Relay>,
    on: bool,
) -> Result<ShellView, String> {
    crate::sync_autostart(&app, on)?;
    relay.update_settings(|s| s.autostart = on)?;
    emit_state(&app);
    Ok(view::build(&app))
}

#[tauri::command]
pub async fn open_setup_docs(app: AppHandle) {
    open_in_browser(&app, RELAY_SETUP_DOCS_URL);
}

#[tauri::command]
pub async fn open_sign_up(app: AppHandle) {
    open_in_browser(&app, SIGN_UP_URL);
}

#[tauri::command]
pub async fn open_log_folder(app: AppHandle) {
    crate::open_log_folder(&app);
}

/// Installs the offered update and restarts.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    crate::updates::install(&app).await
}
