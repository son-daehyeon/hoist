use std::path::Path;
use std::sync::{Arc, Mutex};

use hoist_core::image as cimg;
use hoist_core::model::*;
use hoist_core::store::{Folder, ProjectHandle};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::agent::{self, ChatItem};
use crate::error::{Error, Result};
use crate::img;
use crate::state::{AppState, lock};

#[derive(Serialize, specta::Type)]
pub struct ProjectSummaryView {
    id: String,
    name: String,
    width_mm: f64,
    height_mm: f64,
    updated_at: String,
    thumbnail_png: Option<String>,
    folder: Option<String>,
    deleted_at: Option<String>,
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
    #[serde(default)]
    fit_mode: Option<FitMode>,
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
            folder: p.folder,
            deleted_at: p.deleted_at,
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub fn list_folders(state: State<'_, AppState>) -> Result<Vec<Folder>> {
    Ok(state.store().folders()?)
}

#[tauri::command]
#[specta::specta]
pub fn create_folder(state: State<'_, AppState>, name: String, parent: Option<String>) -> Result<Folder> {
    if name.trim().is_empty() {
        return Err(Error::Invalid("폴더 이름을 넣어 주세요".into()));
    }
    Ok(state.store().create_folder(&name, parent.as_deref())?)
}

#[tauri::command]
#[specta::specta]
pub fn rename_folder(state: State<'_, AppState>, id: String, name: String) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Invalid("폴더 이름을 넣어 주세요".into()));
    }
    Ok(state.store().rename_folder(&id, &name)?)
}

/// A project whose agent turn is running cannot leave: its files are being written.
fn refuse_busy(state: &AppState, ids: &[String]) -> Result<()> {
    let agents = state.agents.lock()?;
    let busy = ids
        .iter()
        .any(|id| agents.get(id).is_some_and(|run| run.is_running()));
    if busy {
        return Err(Error::Invalid(
            "에이전트가 작업 중인 프로젝트는 지울 수 없어요".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn delete_folder(state: State<'_, AppState>, id: String) -> Result<()> {
    let store = state.store();
    let folders = store.folders()?;
    let mut gone = vec![id.clone()];
    let mut i = 0;
    while i < gone.len() {
        let children: Vec<String> = folders
            .iter()
            .filter(|f| f.parent.as_deref() == Some(gone[i].as_str()) && !gone.contains(&f.id))
            .map(|f| f.id.clone())
            .collect();
        gone.extend(children);
        i += 1;
    }
    let inside: Vec<String> = store
        .list()?
        .into_iter()
        .filter(|p| p.folder.as_ref().is_some_and(|f| gone.contains(f)))
        .map(|p| p.id)
        .collect();
    refuse_busy(&state, &inside)?;
    for project in store.delete_folder(&id)? {
        state.forget(&project);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn move_project(state: State<'_, AppState>, id: String, folder: Option<String>) -> Result<()> {
    Ok(state.store().move_project(&id, folder.as_deref())?)
}

#[tauri::command]
#[specta::specta]
pub fn trash_project(state: State<'_, AppState>, id: String) -> Result<()> {
    refuse_busy(&state, std::slice::from_ref(&id))?;
    state.store().trash(&id)?;
    state.forget(&id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn restore_project(state: State<'_, AppState>, id: String) -> Result<()> {
    Ok(state.store().restore(&id)?)
}

#[tauri::command]
#[specta::specta]
pub fn empty_trash(state: State<'_, AppState>) -> Result<()> {
    for id in state.store().empty_trash()? {
        state.forget(&id);
    }
    Ok(())
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
    if let Some(mode) = args.fit_mode {
        h.set_fit_mode(mode)?;
    }
    if let Some(p) = args.original_path.as_deref().filter(|p| !p.is_empty()) {
        h.add_original(Path::new(p))?;
        if let Ok(orig) = cimg::load(&h.original_path(1))
            && let Ok(png) = cimg::to_png_bytes(&cimg::resize_max(&orig, img::THUMBNAIL_PX))
        {
            let _ = h.write_thumbnail(&png);
        }
    } else if let Ok(png) = img::thumbnail_png(&h, &state.fonts) {
        let _ = h.write_thumbnail(&png);
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
    refuse_busy(&state, std::slice::from_ref(&id))?;
    state.forget(&id);
    Ok(state.store().delete(&id)?)
}

#[tauri::command]
#[specta::specta]
pub fn rename_project(state: State<'_, AppState>, id: String, name: String) -> Result<Project> {
    let h = state.handle(&id)?;
    let mut h = lock(&h);
    h.rename(name.trim())?;
    Ok(h.project.clone())
}

#[tauri::command]
#[specta::specta]
pub fn set_fit_mode(state: State<'_, AppState>, id: String, fit_mode: FitMode) -> Result<Project> {
    let h = state.handle(&id)?;
    let mut h = lock(&h);
    h.set_fit_mode(fit_mode)?;
    Ok(h.project.clone())
}

#[tauri::command]
#[specta::specta]
pub fn duplicate_project(state: State<'_, AppState>, id: String, name: String) -> Result<Project> {
    if name.trim().is_empty() {
        return Err(Error::Invalid("복제본 이름을 넣어 주세요".into()));
    }
    let h = state.store().duplicate(&id, name.trim())?;
    Ok(h.project.clone())
}

#[tauri::command]
#[specta::specta]
pub fn open_project(state: State<'_, AppState>, id: String) -> Result<OpenedProject> {
    let h = state.handle(&id)?;
    let h = lock(&h);
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
pub fn show_in_folder(state: State<'_, AppState>, id: String) -> Result<()> {
    let dir = state.store().root().join(&id);
    Ok(tauri_plugin_opener::reveal_item_in_dir(&dir)?)
}
