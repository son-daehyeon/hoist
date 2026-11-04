use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use banner_core::store::{ProjectHandle, ProjectStore};
use banner_core::text::Fonts;
use tauri::Wry;
use tauri_plugin_store::Store;

use crate::agent::AgentRun;
use crate::settings::Settings;

pub struct AppState {
    pub settings_store: Arc<Store<Wry>>,
    pub fonts: Arc<Fonts>,
    pub settings: Mutex<Settings>,
    pub open: Mutex<HashMap<String, Arc<Mutex<ProjectHandle>>>>,
    pub agents: Mutex<HashMap<String, Arc<AgentRun>>>,
}

impl AppState {
    pub fn store(&self) -> ProjectStore {
        let dir = self.settings.lock().expect("settings lock").projects_dir.clone();
        ProjectStore::new(dir)
    }

    pub fn handle(&self, id: &str) -> banner_core::Result<Arc<Mutex<ProjectHandle>>> {
        if let Some(h) = self.open.lock().expect("open lock").get(id) {
            return Ok(h.clone());
        }
        let handle = self.store().open(id)?;
        let arc = Arc::new(Mutex::new(handle));
        self.open
            .lock()
            .expect("open lock")
            .insert(id.to_string(), arc.clone());
        Ok(arc)
    }

    pub fn forget(&self, id: &str) {
        self.open.lock().expect("open lock").remove(id);
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.settings.lock().expect("settings lock").clone()
    }

    pub fn save_settings(&self) -> anyhow::Result<()> {
        self.settings
            .lock()
            .expect("settings lock")
            .save(&self.settings_store)
    }
}
