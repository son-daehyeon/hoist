use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_store::{Store, StoreExt};

const KEYRING_SERVICE: &str = "hoist";
const KEYRING_USER: &str = "openai";
const STORE_FILE: &str = "settings.json";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub projects_dir: PathBuf,
    #[serde(default = "default_agent_model")]
    pub agent_model: String,
    #[serde(default = "default_heavy_model")]
    pub heavy_model: String,
    #[serde(default = "default_cheap_model")]
    pub cheap_model: String,
    #[serde(default = "default_image_model")]
    pub image_model: String,
    #[serde(default)]
    pub openai_key_file_fallback: Option<String>,
}

fn default_agent_model() -> String {
    "gpt-6.1-terra".into()
}
fn default_heavy_model() -> String {
    "gpt-6.1-sol".into()
}
fn default_cheap_model() -> String {
    "gpt-6-luna".into()
}
fn default_image_model() -> String {
    "gpt-image-2.6-sunburst".into()
}

const OLD_IMAGE_DEFAULTS: [&str; 1] = ["gpt-image-2.5-sunburst"];

/// A stored value that was only ever the old default follows the default; a value the user chose stays.
fn migrate_image_model(model: &mut String) -> bool {
    if OLD_IMAGE_DEFAULTS.contains(&model.as_str()) {
        *model = default_image_model();
        true
    } else {
        false
    }
}

#[derive(Serialize, Clone, Debug, specta::Type)]
pub struct SettingsView {
    pub openai_key_set: bool,
    pub projects_dir: String,
    pub agent_model: String,
    pub heavy_model: String,
    pub cheap_model: String,
    pub image_model: String,
}

#[derive(Deserialize, Debug, Default, specta::Type)]
pub struct SettingsPatch {
    pub openai_key: Option<String>,
    pub projects_dir: Option<String>,
    pub agent_model: Option<String>,
    pub heavy_model: Option<String>,
    pub cheap_model: Option<String>,
    pub image_model: Option<String>,
}

impl Settings {
    pub fn load(store: &Store<Wry>, default_projects_dir: PathBuf) -> Settings {
        let entries = Value::Object(store.entries().into_iter().collect());
        let mut settings = serde_json::from_value::<Settings>(entries).unwrap_or(Settings {
            projects_dir: default_projects_dir,
            agent_model: default_agent_model(),
            heavy_model: default_heavy_model(),
            cheap_model: default_cheap_model(),
            image_model: default_image_model(),
            openai_key_file_fallback: None,
        });
        if migrate_image_model(&mut settings.image_model)
            && let Err(e) = settings.save(store)
        {
            log::warn!("image model migration not saved: {e}");
        }
        settings
    }

    pub fn save(&self, store: &Store<Wry>) -> anyhow::Result<()> {
        if let Value::Object(fields) = serde_json::to_value(self)? {
            for (key, value) in fields {
                store.set(key, value);
            }
        }
        store.save()?;
        Ok(())
    }

    pub fn view(&self) -> SettingsView {
        SettingsView {
            openai_key_set: self.openai_key().is_some(),
            projects_dir: self.projects_dir.to_string_lossy().into_owned(),
            agent_model: self.agent_model.clone(),
            heavy_model: self.heavy_model.clone(),
            cheap_model: self.cheap_model.clone(),
            image_model: self.image_model.clone(),
        }
    }

    pub fn openai_key(&self) -> Option<String> {
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            && let Ok(pw) = entry.get_password()
            && !pw.trim().is_empty()
        {
            return Some(pw);
        }
        if let Some(k) = &self.openai_key_file_fallback
            && !k.trim().is_empty()
        {
            return Some(k.clone());
        }
        std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())
    }

    pub fn set_openai_key(&mut self, key: &str) {
        let key = key.trim();
        let stored = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .and_then(|e| {
                if key.is_empty() {
                    e.delete_credential()
                } else {
                    e.set_password(key)
                }
            })
            .is_ok();
        self.openai_key_file_fallback = if stored || key.is_empty() {
            None
        } else {
            Some(key.to_string())
        };
    }
}

pub fn open_store(app: &AppHandle) -> anyhow::Result<Arc<Store<Wry>>> {
    let store = app.store(STORE_FILE)?;
    let legacy = app.path().app_config_dir()?.join(STORE_FILE);
    let current = tauri_plugin_store::resolve_store_path(app, STORE_FILE)?;
    if legacy != current && legacy.is_file() && store.is_empty() {
        let fields: serde_json::Map<String, Value> = serde_json::from_slice(&std::fs::read(&legacy)?)?;
        for (key, value) in fields {
            store.set(key, value);
        }
        store.save()?;
        std::fs::rename(&legacy, legacy.with_extension("json.bak"))?;
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_image_default_moves_to_the_new_one() {
        let mut model = "gpt-image-2.5-sunburst".to_string();
        assert!(migrate_image_model(&mut model));
        assert_eq!(model, "gpt-image-2.6-sunburst");
        let mut chosen = "gpt-image-2.5-flare".to_string();
        assert!(!migrate_image_model(&mut chosen));
        assert_eq!(chosen, "gpt-image-2.5-flare");
    }
}
