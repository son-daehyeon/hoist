use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use ort::environment::Environment;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

pub(crate) const RUNTIME_FILE: &str = "onnxruntime.dll";

type Shared = Arc<Mutex<Session>>;

static ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();
static SESSIONS: Mutex<Vec<(&'static str, Shared)>> = Mutex::new(Vec::new());

pub(crate) fn has(models_dir: &Path, model: &str) -> bool {
    models_dir.join(model).is_file() && models_dir.join(RUNTIME_FILE).is_file()
}

pub(crate) fn session(models_dir: &Path, model: &'static str) -> Result<Shared, String> {
    let env = ENVIRONMENT
        .get_or_init(|| {
            ort::init_from(models_dir.join(RUNTIME_FILE))
                .map_err(|e| e.to_string())?
                .with_name("hoist")
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()?;
    let mut all = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, s)) = all.iter().find(|(m, _)| *m == model) {
        return Ok(s.clone());
    }
    let session = Session::builder(env)
        .map_err(|e| e.to_string())?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?
        .commit_from_file(models_dir.join(model))
        .map_err(|e| e.to_string())?;
    let shared = Arc::new(Mutex::new(session));
    all.push((model, shared.clone()));
    Ok(shared)
}
