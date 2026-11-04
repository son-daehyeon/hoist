use std::path::Path;
use std::sync::{Arc, Mutex};

use hoist_core::image as cimg;
use hoist_core::model::*;
use hoist_core::store::ProjectHandle;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::agent::{self, AgentEvent, ChatItem, SendContext, img, prompt::TurnKind};
use crate::error::{Error, Result};
use crate::state::AppState;

#[derive(Serialize, specta::Type)]
pub struct ProjectSummaryView {
    id: String,
    name: String,
    width_mm: f64,
    height_mm: f64,
    updated_at: String,
    thumbnail_png: Option<String>,
}

#[derive(Serialize, specta::Type)]
pub struct VersionView {
    seq: u64,
    ts: String,
    actor: Actor,
    summary: String,
    turn: Option<String>,
    thumbnail_png: Option<String>,
}

#[derive(Serialize, specta::Type)]
pub struct OpenedProject {
    project: Project,
    document: Document,
    memory: String,
    chat: Vec<ChatItem>,
    versions: Vec<VersionView>,
    can_undo: bool,
    can_redo: bool,
}

#[derive(Deserialize, specta::Type)]
pub struct CreateArgs {
    name: String,
    width_mm: f64,
    height_mm: f64,
    #[serde(default)]
    original_path: Option<String>,
}

fn file_data_url(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| img::data_url(&b))
}

fn versions_view(h: &ProjectHandle) -> Vec<VersionView> {
    h.versions()
        .unwrap_or_default()
        .into_iter()
        .map(|v| VersionView {
            thumbnail_png: file_data_url(&h.version_thumbnail_path(v.seq)),
            seq: v.seq,
            ts: v.ts,
            actor: v.actor,
            summary: v.summary,
            turn: v.turn,
        })
        .collect()
}

pub(super) fn refresh_thumbnail(state: &AppState, h: &mut ProjectHandle) {
    if let Ok(png) = img::thumbnail_png(h, &state.fonts) {
        let _ = h.write_thumbnail(&png);
    }
}

#[tauri::command]
#[specta::specta]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectSummaryView>> {
    let store = state.store();
    std::fs::create_dir_all(store.root())?;
    Ok(store
        .list()?
        .into_iter()
        .map(|p| ProjectSummaryView {
            thumbnail_png: p.thumbnail.as_deref().and_then(file_data_url),
            id: p.id,
            name: p.name,
            width_mm: p.width_mm,
            height_mm: p.height_mm,
            updated_at: p.updated_at,
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub fn create_project(state: State<'_, AppState>, args: CreateArgs) -> Result<Project> {
    if args.name.trim().is_empty() || args.width_mm <= 0.0 || args.height_mm <= 0.0 {
        return Err(Error::Invalid("이름과 크기를 확인해 주세요".into()));
    }
    let store = state.store();
    std::fs::create_dir_all(store.root())?;
    let mut h = store.create(args.name.trim(), args.width_mm, args.height_mm)?;
    if let Some(p) = args.original_path.as_deref().filter(|p| !p.is_empty()) {
        h.add_original(Path::new(p))?;
        if let Ok(orig) = cimg::load(&h.original_path(1))
            && let Ok(png) = cimg::to_png_bytes(&cimg::resize_max(&orig, 640))
        {
            let _ = h.write_thumbnail(&png);
        }
    } else {
        refresh_thumbnail(&state, &mut h);
    }
    let project = h.project.clone();
    state
        .open
        .lock()?
        .insert(project.id.clone(), Arc::new(Mutex::new(h)));
    Ok(project)
}

#[tauri::command]
#[specta::specta]
pub fn delete_project(state: State<'_, AppState>, id: String) -> Result<()> {
    state.forget(&id);
    Ok(state.store().delete(&id)?)
}

#[tauri::command]
#[specta::specta]
pub fn rename_project(state: State<'_, AppState>, id: String, name: String) -> Result<Project> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    h.rename(name.trim())?;
    Ok(h.project.clone())
}

#[tauri::command]
#[specta::specta]
pub fn duplicate_project(state: State<'_, AppState>, id: String) -> Result<Project> {
    let h = state.store().duplicate(&id)?;
    Ok(h.project.clone())
}

#[tauri::command]
#[specta::specta]
pub fn open_project(state: State<'_, AppState>, id: String) -> Result<OpenedProject> {
    let h = state.handle(&id)?;
    let h = img::lock(&h);
    Ok(OpenedProject {
        project: h.project.clone(),
        document: h.document.clone(),
        memory: h.memory().unwrap_or_default(),
        chat: agent::chat_items(&h),
        versions: versions_view(&h),
        can_undo: h.can_undo(),
        can_redo: h.can_redo(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn replace_original(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    path: String,
    on_event: Channel<AgentEvent>,
) -> Result<Project> {
    if state.settings_snapshot().openai_key().is_none() {
        return Err(Error::NoApiKey);
    }
    let h = state.handle(&id)?;
    let project = {
        let mut g = img::lock(&h);
        g.add_original(Path::new(&path))?;
        g.project.clone()
    };
    agent::spawn_turn(
        app,
        id,
        "원본을 새 이미지로 바꿨어요.".into(),
        SendContext::default(),
        TurnKind::ReplaceOriginal,
        on_event,
    );
    Ok(project)
}
