//! The page's commands: a thin layer over the core engine. Every command is `async` so it runs
//! on Tauri's Tokio runtime, which the engine's listener needs.

use relay_core::upstream::auth::{browser_url_ok, device_name_from, PairingView};
use tauri::{AppHandle, State};

use crate::view::{self, ShellView};
use crate::{
    emit_state, open_in_browser, Relay, ADD_DESTINATION_URL, RELAY_SETUP_DOCS_URL, SIGN_UP_URL,
};

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

/// The DX cluster switch: on listens for logging software and opens the spots stream.
#[tauri::command]
pub async fn set_cluster_enabled(
    app: AppHandle,
    relay: State<'_, Relay>,
    on: bool,
) -> Result<ShellView, String> {
    relay.engine.set_cluster_enabled(on).await;
    relay.update_settings(|s| s.cluster_enabled = on)?;
    emit_state(&app);
    Ok(view::build(&app))
}

/// "Change port" on the DX cluster tab. Only the default port falls through to the next free
/// one; a port chosen here is used as is, and one in use shows on the card.
#[tauri::command]
pub async fn set_cluster_port(
    app: AppHandle,
    relay: State<'_, Relay>,
    port: u16,
) -> Result<ShellView, String> {
    if port == 0 {
        return Err("Port must be from 1 to 65535.".into());
    }
    relay.engine.set_cluster_port(port).await;
    relay.update_settings(|s| s.cluster_port = port)?;
    emit_state(&app);
    Ok(view::build(&app))
}

/// "Let other computers on this network connect".
#[tauri::command]
pub async fn set_cluster_lan(
    app: AppHandle,
    relay: State<'_, Relay>,
    on: bool,
) -> Result<ShellView, String> {
    relay.engine.set_cluster_lan(on).await;
    relay.update_settings(|s| s.cluster_lan = on)?;
    emit_state(&app);
    Ok(view::build(&app))
}

/// "Send test spot": one line to every program connected to the DX cluster.
#[tauri::command]
pub async fn send_test_spot(app: AppHandle, relay: State<'_, Relay>) -> Result<ShellView, String> {
    relay.engine.send_test_spot().map_err(|e| e.to_string())?;
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

/// "Add destination" on the DX cluster tab.
#[tauri::command]
pub async fn open_add_destination(app: AppHandle) {
    open_in_browser(&app, ADD_DESTINATION_URL);
}

#[tauri::command]
pub async fn open_log_folder(app: AppHandle) {
    crate::open_log_folder(&app);
}

/// Opens the release page of an update this copy can't install itself (.deb).
#[tauri::command]
pub async fn open_update_page(app: AppHandle) -> Result<(), String> {
    crate::updates::open_release_page(&app)
}

/// Installs the offered update and restarts.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    crate::updates::install(&app).await
}
