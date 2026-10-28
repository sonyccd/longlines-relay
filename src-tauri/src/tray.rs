//! The tray icon: Open, Open log folder, Quit.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

const OPEN: &str = "open";
const LOGS: &str = "logs";
const QUIT: &str = "quit";

pub fn build(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, "Open", true, None::<&str>)?;
    let logs = MenuItem::with_id(app, LOGS, "Open log folder", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &logs, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Long Lines Relay")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN => crate::show_main_window(app),
            LOGS => crate::open_log_folder(app),
            QUIT => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
