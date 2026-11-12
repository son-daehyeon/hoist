mod install;
mod paths;
mod registry;
mod resources;
mod silent;
mod uninstall;

use serde::Serialize;
use tauri::Manager;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Serialize, Clone)]
pub struct Mode {
    mode: String,
    projects_dir: Option<String>,
}

#[tauri::command]
fn mode(app: tauri::AppHandle) -> Mode {
    let uninstalling = uninstall::is_uninstall_mode();
    Mode {
        mode: if uninstalling { "uninstall" } else { "install" }.into(),
        projects_dir: uninstalling
            .then(|| uninstall::projects_dir(&app))
            .flatten()
            .map(|dir| dir.to_string_lossy().into_owned()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Some(flags) = silent::silent_mode() {
        std::process::exit(silent::run_silent(flags));
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.center();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            mode,
            install::plan,
            install::install,
            install::install_resources,
            install::launch,
            uninstall::uninstall,
            install::pick_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running installer");
}
