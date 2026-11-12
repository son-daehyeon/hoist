use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, State};

use crate::error::{Error, Result};
use crate::openai::{KeyCheck, check_key};
use crate::settings::{SettingsPatch, SettingsView};
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn get_settings(state: State<'_, AppState>) -> Result<SettingsView> {
    Ok(state.settings_snapshot().view())
}

fn replace_if_given(current: &mut String, given: Option<String>) {
    if let Some(value) = given.filter(|v| !v.trim().is_empty()) {
        *current = value;
    }
}

fn moved_dir(current: &Path, given: Option<String>) -> Option<PathBuf> {
    given
        .map(|d| PathBuf::from(d.trim()))
        .filter(|d| !d.as_os_str().is_empty() && d != current)
}

#[derive(serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    Ok,
    Missing,
    Rejected,
}

#[tauri::command]
#[specta::specta]
pub async fn api_key_status(state: State<'_, AppState>) -> Result<KeyStatus> {
    let Some(key) = state.settings_snapshot().openai_key() else {
        return Ok(KeyStatus::Missing);
    };
    Ok(match check_key(&key).await {
        KeyCheck::Rejected => KeyStatus::Rejected,
        KeyCheck::Valid | KeyCheck::Unreachable => KeyStatus::Ok,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<SettingsView> {
    if let Some(key) = patch.openai_key.as_deref().filter(|k| !k.is_empty()) {
        match check_key(key).await {
            KeyCheck::Valid => {}
            KeyCheck::Rejected => return Err(Error::Invalid("API 키가 맞지 않아요".into())),
            KeyCheck::Unreachable => {
                return Err(Error::Invalid(
                    "API 키를 확인하지 못했어요. 인터넷 연결을 확인해 주세요".into(),
                ));
            }
        }
    }
    let moved = {
        let mut s = state.settings.lock()?;
        if let Some(k) = patch.openai_key {
            s.set_openai_key(&k);
        }
        let dir = moved_dir(&s.projects_dir, patch.projects_dir);
        if let Some(d) = &dir {
            app.asset_protocol_scope()
                .allow_directory(d, true)
                .map_err(anyhow::Error::from)?;
            s.projects_dir = d.clone();
        }
        replace_if_given(&mut s.agent_model, patch.agent_model);
        replace_if_given(&mut s.heavy_model, patch.heavy_model);
        replace_if_given(&mut s.cheap_model, patch.cheap_model);
        replace_if_given(&mut s.image_model, patch.image_model);
        dir.is_some()
    };
    state.save_settings()?;
    if moved {
        state.forget_idle();
    }
    Ok(state.settings_snapshot().view())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_the_same_folder_is_not_a_move() {
        let current = Path::new(r"C:\Users\me\Documents\Hoist");
        let given = |d: &str| moved_dir(current, Some(d.to_string()));
        assert_eq!(moved_dir(current, None), None);
        assert_eq!(given(r"C:\Users\me\Documents\Hoist"), None);
        assert_eq!(given(r"C:\Users\me\Documents\Hoist\"), None);
        assert_eq!(given(r" C:\Users\me\Documents\Hoist "), None);
        assert_eq!(given("  "), None);
        assert_eq!(given(""), None);
        assert_eq!(given(r"D:\banners"), Some(PathBuf::from(r"D:\banners")));
    }
}
