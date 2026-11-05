use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::error::Result;
use crate::settings::{SettingsPatch, SettingsView};
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn get_settings(state: State<'_, AppState>) -> Result<SettingsView> {
    Ok(state.settings_snapshot().view())
}

#[tauri::command]
#[specta::specta]
pub fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<SettingsView> {
    {
        let mut s = state.settings.lock()?;
        if let Some(k) = patch.openai_key {
            s.set_openai_key(&k);
        }
        if let Some(d) = patch.projects_dir.filter(|d| !d.trim().is_empty()) {
            s.projects_dir = PathBuf::from(d.trim());
            app.asset_protocol_scope()
                .allow_directory(&s.projects_dir, true)
                .map_err(anyhow::Error::from)?;
        }
        if let Some(m) = patch.agent_model.filter(|m| !m.trim().is_empty()) {
            s.agent_model = m;
        }
        if let Some(m) = patch.heavy_model.filter(|m| !m.trim().is_empty()) {
            s.heavy_model = m;
        }
        if let Some(m) = patch.cheap_model.filter(|m| !m.trim().is_empty()) {
            s.cheap_model = m;
        }
        if let Some(m) = patch.image_model.filter(|m| !m.trim().is_empty()) {
            s.image_model = m;
        }
    }
    state.save_settings()?;
    state.open.lock()?.clear();
    Ok(state.settings_snapshot().view())
}
