mod agent;
mod commands;
mod error;
mod openai;
mod settings;
mod state;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};

use hoist_core::text::Fonts;
use specta_typescript::semantic;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_specta::{ErrorHandlingMode, collect_commands, collect_events};

use settings::Settings;
use state::AppState;

#[cfg(any(debug_assertions, test))]
const BINDINGS_PATH: &str = "../src/shared/api/bindings.ts";

fn builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::list_projects,
            commands::list_folders,
            commands::create_folder,
            commands::rename_folder,
            commands::delete_folder,
            commands::move_project,
            commands::trash_project,
            commands::restore_project,
            commands::empty_trash,
            commands::create_project,
            commands::delete_project,
            commands::rename_project,
            commands::set_fit_mode,
            commands::duplicate_project,
            commands::open_project,
            commands::show_in_folder,
            commands::apply_ops,
            commands::path_boolean,
            commands::outline_text,
            commands::import_image,
            commands::remove_background,
            commands::undo,
            commands::redo,
            commands::restore_version,
            commands::render_layers,
            commands::layer_svgs,
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
            commands::list_fonts,
            commands::update_check,
            commands::update_install,
        ])
        .events(collect_events![commands::UpdateAvailable])
        .error_handling(ErrorHandlingMode::Throw)
        .dangerously_cast_bigints_to_number()
        .typ::<error::ApiError>()
        .semantic_types(
            semantic::Configuration::default()
                .enable_lossless_floats()
                .define::<commands::ExportScale>(|_| specta_typescript::define("1 | 10").into(), None, None),
        )
}

#[cfg(any(debug_assertions, test))]
fn export_bindings(builder: &tauri_specta::Builder<tauri::Wry>) -> Result<(), specta_typescript::Error> {
    builder.export(specta_typescript::Typescript::default(), BINDINGS_PATH)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta = builder();
    #[cfg(debug_assertions)]
    export_bindings(&specta).expect("export typescript bindings");
    #[cfg(debug_assertions)]
    let devtools = tauri_plugin_devtools::init();

    let app = tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        show_main(app)
    }));
    #[cfg(debug_assertions)]
    let app = app.plugin(devtools);
    #[cfg(not(debug_assertions))]
    let app = app.plugin(
        tauri_plugin_log::Builder::new()
            .level(log::LevelFilter::Info)
            .build(),
    );
    let app = app
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_decorum::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());
    #[cfg(not(debug_assertions))]
    let app = app.plugin(
        tauri_plugin_prevent_default::Builder::new()
            .with_flags(tauri_plugin_prevent_default::Flags::all())
            .build(),
    );

    static ANNOUNCE_UPDATE: Once = Once::new();
    app.invoke_handler(specta.invoke_handler())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && payload.event() == PageLoadEvent::Finished {
                let app = webview.app_handle().clone();
                ANNOUNCE_UPDATE.call_once(|| {
                    tauri::async_runtime::spawn(commands::announce_update(app));
                });
            }
        })
        .setup(move |app| {
            specta.mount_events(app);
            let config_dir = app.path().app_config_dir()?;
            let default_projects = app
                .path()
                .document_dir()
                .map(|d| d.join("Hoist"))
                .unwrap_or_else(|_| config_dir.join("projects"));
            let settings_store = settings::open_store(app.handle())?;
            let settings = Settings::load(&settings_store, default_projects);
            let _ = std::fs::create_dir_all(&settings.projects_dir);
            app.asset_protocol_scope()
                .allow_directory(&settings.projects_dir, true)?;
            let bundled = app
                .path()
                .resource_dir()
                .ok()
                .map(|r| r.join("fonts"))
                .filter(|p| p.is_dir())
                .or_else(|| {
                    let dev = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts");
                    dev.is_dir().then_some(dev)
                });
            let fonts = Arc::new(Fonts::new(bundled.as_deref()));
            app.manage(AppState {
                settings_store,
                fonts,
                models_dir: commands::models_dir(app.handle()),
                settings: Mutex::new(settings),
                open: Mutex::new(HashMap::new()),
                agents: Mutex::new(HashMap::new()),
                thumbnail_generation: Mutex::new(HashMap::new()),
            });
            #[cfg(windows)]
            disable_webview_autofill(app);
            build_tray(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Closing the window only hides it, like a messenger: the app stays in the tray, so an agent turn that takes
/// ten minutes keeps running. The tray icon brings the window back (left click or "Hoist 열기") and is the one
/// place that really quits.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Hoist 열기", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Hoist")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Edge's WebView2 sometimes ignores `autocomplete="off"`; these settings are only reachable through its COM interface.
#[cfg(windows)]
#[allow(unsafe_code)]
fn disable_webview_autofill(app: &tauri::App) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
    use windows_core::Interface;

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.with_webview(|webview| unsafe {
        let Ok(settings) = webview
            .controller()
            .CoreWebView2()
            .and_then(|core| core.Settings())
        else {
            return;
        };
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings4>() {
            let _ = settings.SetIsGeneralAutofillEnabled(false);
            let _ = settings.SetIsPasswordAutosaveEnabled(false);
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn export_bindings() {
        super::export_bindings(&super::builder()).expect("export typescript bindings");
    }
}
