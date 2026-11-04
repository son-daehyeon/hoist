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
    #[serde(default = "d_agent")]
    pub agent_model: String,
    #[serde(default = "d_heavy")]
    pub heavy_model: String,
    #[serde(default = "d_cheap")]
    pub cheap_model: String,
    #[serde(default = "d_image")]
    pub image_model: String,
    #[serde(default)]
    pub usage_month_krw: f64,
    #[serde(default)]
    pub usage_month: String,
    #[serde(default)]
    pub openai_key_file_fallback: Option<String>,
}

fn d_agent() -> String {
    "gpt-6.1-terra".into()
}
fn d_heavy() -> String {
    "gpt-6.1-sol".into()
}
fn d_cheap() -> String {
    "gpt-6-luna".into()
}
fn d_image() -> String {
    "gpt-image-2.5-sunburst".into()
}

#[derive(Serialize, Clone, Debug, specta::Type)]
pub struct SettingsView {
    pub openai_key_set: bool,
    pub projects_dir: String,
    pub agent_model: String,
    pub heavy_model: String,
    pub cheap_model: String,
    pub image_model: String,
    pub usage_month_krw: f64,
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
        serde_json::from_value::<Settings>(entries).unwrap_or(Settings {
            projects_dir: default_projects_dir,
            agent_model: d_agent(),
            heavy_model: d_heavy(),
            cheap_model: d_cheap(),
            image_model: d_image(),
            usage_month_krw: 0.0,
            usage_month: String::new(),
            openai_key_file_fallback: None,
        })
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
            usage_month_krw: self.usage_month_krw,
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

    pub fn add_usage_krw(&mut self, krw: f64) {
        let month = chrono::Utc::now().format("%Y-%m").to_string();
        if self.usage_month != month {
            self.usage_month = month;
            self.usage_month_krw = 0.0;
        }
        self.usage_month_krw += krw;
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
