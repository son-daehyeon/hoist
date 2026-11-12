mod agent;
mod commands;
mod error;
mod img;
mod openai;
mod popups;
mod settings;
mod state;

use std::sync::Once;

use specta_typescript::semantic;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::PageLoadEvent;
use tauri::{Manager, WindowEvent};
use tauri_specta::{ErrorHandlingMode, collect_commands, collect_events};

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
            commands::api_key_status,
            commands::pick_image_file,
            commands::pick_folder,
            commands::pick_save_path,
            commands::read_image_info,
            commands::tutorial_sample,
            commands::list_fonts,
            commands::update_check,
            commands::update_install,
            popups::popup_busy_projects,
            popups::popup_open,
            popups::popup_hide,
            popups::popup_quit,
        ])
        .events(collect_events![
            commands::UpdateAvailable,
            popups::Notice,
            popups::OpenProject,
            popups::PopupShown
        ])
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
        popups::show_main(app)
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
        // the popups are placed by the app each time they open; remembering them would bring them back visible
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&[popups::TRAY, popups::NOTICE])
                .build(),
        )
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
        .on_window_event(on_window_event)
        .on_page_load(|webview, payload| {
            if webview.label() == popups::MAIN && payload.event() == PageLoadEvent::Finished {
                let app = webview.app_handle().clone();
                ANNOUNCE_UPDATE.call_once(|| {
                    tauri::async_runtime::spawn(commands::announce_update(app));
                });
            }
        })
        .setup(move |app| {
            specta.mount_events(app);
            app.manage(AppState::new(app)?);
            #[cfg(windows)]
            disable_webview_autofill(app);
            // the app is usable without its tray menu and notices, so their windows failing must not stop it
            if let Err(e) = popups::build(app) {
                log::warn!("popup windows: {e}");
            }
            build_tray(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } if window.label() == popups::MAIN => {
            api.prevent_close();
            let _ = window.hide();
        }
        WindowEvent::Focused(false) if window.label() == popups::TRAY => {
            let _ = window.hide();
        }
        _ => {}
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Hoist")
        .on_tray_icon_event(|tray, event| {
            let TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            else {
                return;
            };
            match button {
                MouseButton::Left => popups::show_main(tray.app_handle()),
                MouseButton::Right => popups::show_tray_menu(tray.app_handle(), position),
                _ => {}
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

    let Some(window) = app.get_webview_window(popups::MAIN) else {
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
