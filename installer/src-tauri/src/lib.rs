mod install;

use serde::Serialize;
use tauri::Manager;

#[derive(Serialize, Clone)]
pub struct Mode {
    mode: String,
    version: String,
}

#[tauri::command]
fn mode() -> Mode {
    Mode { mode: if install::is_uninstall_mode() { "uninstall".into() } else { "install".into() }, version: env!("CARGO_PKG_VERSION").into() }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.center();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![mode, install::plan, install::install, install::launch, install::uninstall, install::pick_dir, install::close_app])
        .run(tauri::generate_context!())
        .expect("error while running installer");
}
