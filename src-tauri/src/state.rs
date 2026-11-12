use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use hoist_core::matting;
use hoist_core::store::{ProjectHandle, ProjectStore};
use hoist_core::text::Fonts;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_store::Store;

use crate::agent::AgentRun;
use crate::settings::{self, Settings};

pub struct AppState {
    pub settings_store: Arc<Store<Wry>>,
    pub fonts: Arc<Fonts>,
    pub models_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub open: Mutex<HashMap<String, Arc<Mutex<ProjectHandle>>>>,
    pub agents: Mutex<HashMap<String, Arc<AgentRun>>>,
    pub thumbnail_generation: Mutex<HashMap<String, u64>>,
}

pub fn models_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .resource_dir()
        .ok()
        .map(|r| r.join("models"))
        .filter(|p| matting::available(p))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models"))
}

fn keep_running(
    open: &mut HashMap<String, Arc<Mutex<ProjectHandle>>>,
    agents: &HashMap<String, Arc<AgentRun>>,
) {
    open.retain(|id, _| agents.get(id).is_some_and(|run| run.is_running()));
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(app: &tauri::App) -> Result<Self, Box<dyn std::error::Error>> {
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
                let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts");
                dev.is_dir().then_some(dev)
            });
        let fonts = Arc::new(Fonts::new(bundled.as_deref()));
        Ok(AppState {
            settings_store,
            fonts,
            models_dir: models_dir(app.handle()),
            settings: Mutex::new(settings),
            open: Mutex::new(HashMap::new()),
            agents: Mutex::new(HashMap::new()),
            thumbnail_generation: Mutex::new(HashMap::new()),
        })
    }

    pub fn store(&self) -> ProjectStore {
        let dir = self.settings.lock().expect("settings lock").projects_dir.clone();
        ProjectStore::new(dir)
    }

    pub fn handle(&self, id: &str) -> hoist_core::Result<Arc<Mutex<ProjectHandle>>> {
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

    pub fn forget_idle(&self) {
        let agents = lock(&self.agents);
        keep_running(&mut lock(&self.open), &agents);
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    #[test]
    fn a_running_turn_keeps_its_handle_through_a_folder_change() {
        let root = std::env::temp_dir().join(format!("hoist-open-{}", uuid::Uuid::new_v4().simple()));
        let store = ProjectStore::new(root);
        let mut open = HashMap::new();
        let mut agents = HashMap::new();
        for name in ["busy", "idle"] {
            let h = store.create(name, 600.0, 300.0).expect("project");
            let run = AgentRun::new();
            run.running.store(name == "busy", Ordering::SeqCst);
            agents.insert(h.project.id.clone(), Arc::new(run));
            open.insert(h.project.id.clone(), Arc::new(Mutex::new(h)));
        }
        let closed = store.create("closed", 600.0, 300.0).expect("project");
        open.insert(closed.project.id.clone(), Arc::new(Mutex::new(closed)));
        let busy = open
            .values()
            .find(|h| lock(h).project.name == "busy")
            .expect("busy")
            .clone();

        keep_running(&mut open, &agents);

        assert_eq!(open.len(), 1);
        assert!(open.values().all(|h| Arc::ptr_eq(h, &busy)));
    }
}
