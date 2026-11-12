use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder};
use tauri_specta::Event;

use crate::error::Result;
use crate::state::AppState;

pub const MAIN: &str = "main";
pub const TRAY: &str = "tray";
pub const NOTICE: &str = "notice";
/// Logical sizes; each page leaves a margin inside for its own drop shadow.
const TRAY_SIZE: (f64, f64) = (232.0, 168.0);
const NOTICE_SIZE: (f64, f64) = (372.0, 108.0);
const SCREEN_MARGIN: f64 = 12.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    Done,
    Stopped,
    Question,
}

/// What the notice window shows. Sent to that window just before it appears.
#[derive(Serialize, Deserialize, Clone, Debug, specta::Type, tauri_specta::Event)]
pub struct Notice {
    pub project_id: String,
    pub project_name: String,
    pub kind: NoticeKind,
}

/// Asks the main window to show a project, from a notice or the tray menu.
#[derive(Serialize, Deserialize, Clone, Debug, specta::Type, tauri_specta::Event)]
pub struct OpenProject {
    pub id: String,
}

/// Tells a popup it is about to be shown, so it reloads what it displays.
#[derive(Serialize, Deserialize, Clone, Debug, specta::Type, tauri_specta::Event)]
pub struct PopupShown;

pub fn build(app: &tauri::App) -> tauri::Result<()> {
    for (label, page, (w, h), focusable) in [
        (TRAY, "tray.html", TRAY_SIZE, true),
        (NOTICE, "notice.html", NOTICE_SIZE, false),
    ] {
        WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
            .title("Hoist")
            .inner_size(w, h)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(false)
            .focusable(focusable)
            .build()?;
    }
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn show_tray_menu(app: &AppHandle, at: PhysicalPosition<f64>) {
    let Some(window) = app.get_webview_window(TRAY) else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let (w, h) = (TRAY_SIZE.0 * scale, TRAY_SIZE.1 * scale);
    let origin = app
        .monitor_from_point(at.x, at.y)
        .ok()
        .flatten()
        .map(|m| (f64::from(m.position().x), f64::from(m.position().y)))
        .unwrap_or((0.0, 0.0));
    let position = PhysicalPosition::new((at.x - w).max(origin.0), (at.y - h).max(origin.1));
    let _ = window.set_position(position);
    let _ = PopupShown.emit_to(app, TRAY);
    let _ = window.show();
    let _ = window.set_focus();
}

/// Runs on the main thread: Tauri 2.12 emits while holding its webview list lock and waits for the main
/// thread, whose window events take that same lock, so an emit from a worker (agent turns) freezes the app.
pub fn notify(app: &AppHandle, notice: Notice) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || show_notice(&handle, notice)) {
        log::warn!("notice not shown: {e}");
    }
}

fn show_notice(app: &AppHandle, notice: Notice) {
    let watching = app
        .get_webview_window(MAIN)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
    if watching {
        return;
    }
    let Some(window) = app.get_webview_window(NOTICE) else {
        return;
    };
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let x =
            f64::from(area.position.x) + f64::from(area.size.width) - (NOTICE_SIZE.0 + SCREEN_MARGIN) * scale;
        let y = f64::from(area.position.y) + f64::from(area.size.height)
            - (NOTICE_SIZE.1 + SCREEN_MARGIN) * scale;
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    let _ = notice.emit_to(app, NOTICE);
    let _ = window.show();
}

/// The projects whose agent is working right now, for the tray menu.
#[tauri::command]
#[specta::specta]
pub fn popup_busy_projects(state: State<'_, AppState>) -> Result<Vec<String>> {
    let running: Vec<String> = state
        .agents
        .lock()?
        .iter()
        .filter(|(_, run)| run.is_running())
        .map(|(id, _)| id.clone())
        .collect();
    Ok(state
        .store()
        .list()?
        .into_iter()
        .filter(|p| running.contains(&p.id))
        .map(|p| p.name)
        .collect())
}

/// Brings the main window back, on a project if one is named, and hides the popup that asked.
#[tauri::command]
#[specta::specta]
pub fn popup_open(app: AppHandle, window: tauri::Window, project: Option<String>) {
    let _ = window.hide();
    show_main(&app);
    if let Some(id) = project {
        let _ = OpenProject { id }.emit_to(&app, MAIN);
    }
}

#[tauri::command]
#[specta::specta]
pub fn popup_hide(window: tauri::Window) {
    let _ = window.hide();
}

#[tauri::command]
#[specta::specta]
pub fn popup_quit(app: AppHandle) {
    app.exit(0);
}
