//! Long Lines Relay's desktop shell.
//!
//! Everything the relay does lives in `relay-core`. This crate starts the core engine, keeps the
//! window and the tray, and turns commands from the page into engine calls. After every engine
//! event it sends the page the whole [`view::ShellView`] as the `state` event.

mod commands;
mod tray;
mod updates;
mod view;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use relay_core::engine::{Engine, EngineConfig, EngineError};
use relay_core::logging::WorkerGuard;
use relay_core::settings::{Settings, SETTINGS_FILE};
use relay_core::upstream::auth::device_name_from;
use relay_core::upstream::client::{base_url_from_env, ClientConfig};
use relay_core::upstream::token_store::{CachingTokenStore, KeyringTokenStore};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

/// "Set up logging software" opens this page.
pub const RELAY_SETUP_DOCS_URL: &str =
    "https://github.com/sonyccd/longlines-relay/blob/main/docs/loggers.md";
/// "Create one on Long Lines". The web app's sign-in page has the Create account tab.
pub const SIGN_UP_URL: &str = "https://longlines.io/signin";
/// "Add destination" on the DX cluster tab: the web app's new-destination page, on its Local DX
/// cluster type.
pub const ADD_DESTINATION_URL: &str = "https://longlines.io/destinations/new?type=local_cluster";
/// Passed by the login item so the relay starts in the tray without showing its window.
pub const HIDDEN_ARG: &str = "--hidden";
/// The one window's label (`tauri.conf.json`).
pub const MAIN_WINDOW: &str = "main";
/// The event that carries [`view::ShellView`] to the page.
pub const STATE_EVENT: &str = "state";
/// The outgoing queue's file in the app's data folder.
const QUEUE_FILE: &str = "queue.sqlite3";

/// What the shell keeps for the life of the app.
pub struct Relay {
    engine: Engine,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    log_dir: PathBuf,
    updates: updates::Updates,
    /// Flushes the log file when the app exits.
    _log_guard: Option<WorkerGuard>,
}

impl Relay {
    fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Changes the settings and writes them to disk.
    fn update_settings(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut settings = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut settings);
        settings.save(&self.settings_path).map_err(|e| {
            tracing::error!(error = %e, "couldn't save the settings");
            e.to_string()
        })?;
        Ok(settings.clone())
    }
}

/// Sends the page the current state.
pub fn emit_state(app: &AppHandle) {
    let view = view::build(app);
    if let Err(e) = app.emit(STATE_EVENT, view) {
        tracing::warn!(error = %e, "couldn't send the state to the window");
    }
}

/// Shows, unminimizes and focuses the window.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Opens a URL in the default browser, logging (never showing) a failure.
pub fn open_in_browser(app: &AppHandle, url: &str) {
    if let Err(e) = app.opener().open_url(url, None::<&str>) {
        tracing::warn!(error = %e, "couldn't open the browser");
    }
}

/// Opens the folder that holds the log files.
pub fn open_log_folder(app: &AppHandle) {
    let dir = app.state::<Relay>().log_dir.clone();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "couldn't create the log folder");
    }
    if let Err(e) = app
        .opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
    {
        tracing::warn!(error = %e, "couldn't open the log folder");
    }
}

/// Makes the OS login item match "Start when I log in". Development builds never register one,
/// so a debug binary is not started at login.
fn sync_autostart(app: &AppHandle, wanted: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let launcher = app.autolaunch();
    let enabled = launcher.is_enabled().map_err(|e| e.to_string())?;
    let result = match (enabled, wanted) {
        (false, true) => launcher.enable(),
        (true, false) => launcher.disable(),
        _ => Ok(()),
    };
    result.map_err(|e| {
        tracing::warn!(error = %e, "couldn't change the login item");
        e.to_string()
    })
}

/// Moves a queue file that can't be opened (and its WAL files) out of the way, keeping it for
/// diagnosis, so the relay can start with an empty queue.
fn set_queue_aside(queue: &Path) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    for suffix in ["", "-wal", "-shm"] {
        let mut from = queue.as_os_str().to_owned();
        from.push(suffix);
        let from = PathBuf::from(from);
        if !from.exists() {
            continue;
        }
        let mut to = queue.as_os_str().to_owned();
        to.push(format!(".unreadable-{stamp}{suffix}"));
        let to = PathBuf::from(to);
        match std::fs::rename(&from, &to) {
            Ok(()) => {
                tracing::warn!(from = %from.display(), to = %to.display(), "set the queue file aside");
            }
            Err(e) => {
                tracing::error!(error = %e, path = %from.display(), "couldn't set the queue file aside");
            }
        }
    }
}

/// Forwards engine events to the page. A burst of events sends one state.
fn spawn_bridge(app: AppHandle) {
    let mut events = app.state::<Relay>().engine.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
            loop {
                match events.try_recv() {
                    Ok(_) | Err(TryRecvError::Lagged(_)) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Closed) => return,
                }
            }
            emit_state(&app);
        }
    });
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let paths = app.path();
    let log_dir = paths.app_log_dir()?;
    let log_guard = match relay_core::logging::init(&log_dir) {
        Ok(guard) => Some(guard),
        Err(e) => {
            eprintln!("Long Lines Relay: logging is off: {e}");
            None
        }
    };
    let settings_path = paths.app_config_dir()?.join(SETTINGS_FILE);
    let data_dir = paths.app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let settings = Settings::load_or_default(&settings_path);
    let version = app.package_info().version.to_string();
    tracing::info!(%version, port = settings.port, "Long Lines Relay starting");

    let tokens = Arc::new(CachingTokenStore::new(KeyringTokenStore::new()));
    let queue_path = data_dir.join(QUEUE_FILE);
    // The DX cluster's greeting names the computer until `GET /me` gives the paired name.
    let device_name = device_name_from(&gethostname::gethostname().to_string_lossy());
    let config = || {
        let client = ClientConfig::new(base_url_from_env(), version.clone());
        let mut config =
            EngineConfig::new(settings.port, queue_path.clone(), client, tokens.clone());
        config.cluster_enabled = settings.cluster_enabled;
        config.cluster_port = settings.cluster_port;
        config.cluster_lan = settings.cluster_lan;
        config.device_name = device_name.clone();
        config
    };
    // The engine spawns its tasks on Tauri's Tokio runtime.
    let engine = match tauri::async_runtime::block_on(Engine::start(config())) {
        Ok(engine) => engine,
        Err(EngineError::Queue(e)) => {
            // A queue that can't be opened must not keep the relay from starting.
            tracing::error!(error = %e, "couldn't open the outgoing queue; starting a new one");
            set_queue_aside(&queue_path);
            tauri::async_runtime::block_on(Engine::start(config()))?
        }
        Err(e) => return Err(e.into()),
    };

    if let Err(e) = sync_autostart(&handle, settings.autostart) {
        tracing::warn!(error = %e, "couldn't match the login item to the settings");
    }
    app.manage(Relay {
        engine,
        settings: Mutex::new(settings),
        settings_path,
        log_dir,
        updates: updates::Updates::default(),
        _log_guard: log_guard,
    });

    tray::build(app)?;
    spawn_bridge(handle.clone());
    updates::spawn_checks(handle.clone());

    if !std::env::args().any(|arg| arg == HIDDEN_ARG) {
        show_main_window(&handle);
    }
    Ok(())
}

/// Runs the app until Quit.
pub fn run() {
    let app = tauri::Builder::default()
        // First, so a second launch hands over before anything else starts (the port included).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .setup(setup)
        .on_window_event(|window, event| {
            // Closing the window hides it; the relay keeps running in the tray until Quit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == MAIN_WINDOW {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::start_sign_in,
            commands::open_sign_in_page,
            commands::cancel_sign_in,
            commands::sign_out,
            commands::set_test_mode,
            commands::set_port,
            commands::set_autostart,
            commands::set_cluster_enabled,
            commands::set_cluster_port,
            commands::set_cluster_lan,
            commands::send_test_spot,
            commands::open_setup_docs,
            commands::open_sign_up,
            commands::open_add_destination,
            commands::open_log_folder,
            commands::install_update,
            commands::open_update_page,
        ])
        .build(tauri::generate_context!())
        .expect("Long Lines Relay couldn't start");

    app.run(|app, event| {
        // macOS: clicking the Dock icon while the window is hidden shows it again.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main_window(app);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
}
