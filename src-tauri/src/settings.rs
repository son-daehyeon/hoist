use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "banner-studio";
const KEYRING_USER: &str = "openai";

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

#[derive(Serialize, Clone, Debug)]
pub struct SettingsView {
    pub openai_key_set: bool,
    pub projects_dir: String,
    pub agent_model: String,
    pub heavy_model: String,
    pub cheap_model: String,
    pub image_model: String,
    pub usage_month_krw: f64,
}

#[derive(Deserialize, Debug, Default)]
pub struct SettingsPatch {
    pub openai_key: Option<String>,
    pub projects_dir: Option<String>,
    pub agent_model: Option<String>,
    pub heavy_model: Option<String>,
    pub cheap_model: Option<String>,
    pub image_model: Option<String>,
}

impl Settings {
    pub fn load(config_dir: &Path, default_projects_dir: PathBuf) -> Settings {
        let path = config_dir.join("settings.json");
        std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or(Settings {
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

    pub fn save(&self, config_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        std::fs::write(config_dir.join("settings.json"), serde_json::to_vec_pretty(self)?)?;
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
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            if let Ok(pw) = entry.get_password() {
                if !pw.trim().is_empty() {
                    return Some(pw);
                }
            }
        }
        if let Some(k) = &self.openai_key_file_fallback {
            if !k.trim().is_empty() {
                return Some(k.clone());
            }
        }
        std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.trim().is_empty())
    }

    pub fn set_openai_key(&mut self, key: &str) {
        let key = key.trim();
        let stored = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .and_then(|e| if key.is_empty() { e.delete_credential() } else { e.set_password(key) })
            .is_ok();
        self.openai_key_file_fallback = if stored || key.is_empty() { None } else { Some(key.to_string()) };
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
