mod agent;
mod commands;
mod openai;
mod settings;
mod state;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use banner_core::text::Fonts;
use tauri::Manager;

use settings::Settings;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let default_projects = app.path().document_dir().map(|d| d.join("BannerStudio")).unwrap_or_else(|_| config_dir.join("projects"));
            let settings = Settings::load(&config_dir, default_projects);
            let _ = std::fs::create_dir_all(&settings.projects_dir);
            let bundled = app.path().resource_dir().ok().map(|r| r.join("fonts")).filter(|p| p.is_dir()).or_else(|| { let dev = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"); dev.is_dir().then_some(dev) });
            let fonts = Arc::new(Fonts::new(bundled.as_deref()));
            app.manage(AppState { config_dir, fonts, settings: Mutex::new(settings), open: Mutex::new(HashMap::new()), agents: Mutex::new(HashMap::new()) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::create_project,
            commands::delete_project,
            commands::rename_project,
            commands::duplicate_project,
            commands::open_project,
            commands::show_in_folder,
            commands::apply_ops,
            commands::undo,
            commands::redo,
            commands::restore_version,
            commands::render_layers,
            commands::render_original,
            commands::render_composite,
            commands::export_project,
            commands::export_check,
            commands::replace_original,
            commands::agent_send,
            commands::agent_stop,
            commands::agent_answer,
            commands::get_settings,
            commands::set_settings,
            commands::pick_image_file,
            commands::pick_folder,
            commands::pick_save_path,
            commands::read_image_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
