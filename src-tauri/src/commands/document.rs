use std::time::Duration;

use hoist_core::model::*;
use hoist_core::store::ProjectHandle;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::agent::{self, img};
use crate::error::Result;
use crate::state::AppState;

#[derive(Serialize, specta::Type)]
pub struct ApplyResult {
    document: Document,
    entries: Vec<OpEntry>,
    can_undo: bool,
    can_redo: bool,
}

const THUMBNAIL_DELAY: Duration = Duration::from_millis(1500);

fn schedule_thumbnail(app: &AppHandle, id: &str) {
    let generation = {
        let state = app.state::<AppState>();
        let mut map = state
            .thumbnail_generation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let g = map.entry(id.to_string()).or_insert(0);
        *g += 1;
        *g
    };
    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(THUMBNAIL_DELAY).await;
        let latest = app
            .state::<AppState>()
            .thumbnail_generation
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .copied();
        if latest != Some(generation) {
            return;
        }
        let _ = tokio::task::spawn_blocking(move || write_thumbnails(&app.state::<AppState>(), &id)).await;
    });
}

fn write_thumbnails(state: &AppState, id: &str) {
    let Ok(handle) = state.handle(id) else {
        return;
    };
    let h = img::lock(&handle);
    let png = match img::thumbnail_png(&h, &state.fonts) {
        Ok(png) => png,
        Err(e) => {
            log::warn!("thumbnail for {id} failed: {e}");
            return;
        }
    };
    let _ = h.write_thumbnail(&png);
    if let Some(latest) = h.versions().ok().and_then(|v| v.into_iter().next()) {
        let path = h.version_thumbnail_path(latest.seq);
        if !path.exists() {
            let _ = std::fs::write(path, &png);
        }
    }
}

fn apply_result(h: &ProjectHandle, entries: Vec<OpEntry>) -> ApplyResult {
    ApplyResult {
        document: h.document.clone(),
        entries,
        can_undo: h.can_undo(),
        can_redo: h.can_redo(),
    }
}

#[tauri::command]
#[specta::specta]
pub fn apply_ops(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    mut ops: Vec<Op>,
    actor: Actor,
) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    for op in &mut ops {
        if let Op::UpdateLayer { id, patch } = op {
            agent::tools::fit_typed_text(&state.fonts, &h.document, patch, id);
        }
    }
    apply_and_record(&app, &id, &mut h, ops, actor)
}

pub(super) fn apply_and_record(
    app: &AppHandle,
    id: &str,
    h: &mut ProjectHandle,
    ops: Vec<Op>,
    actor: Actor,
) -> Result<ApplyResult> {
    let entries = h.apply_ops(ops, actor, None)?;
    let recent_user = h
        .versions()
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|v| {
            v.actor == Actor::User
                && chrono::DateTime::parse_from_rfc3339(&v.ts)
                    .map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds() < 60)
                    .unwrap_or(false)
        })
        .unwrap_or(false);
    if !recent_user {
        let summary = format!(
            "직접 편집 · {}",
            entries
                .first()
                .map(|e| agent::describe_entry(e, &h.document))
                .unwrap_or_default()
        );
        let _ = h.snapshot(Actor::User, &summary, None, None);
    }
    schedule_thumbnail(app, id);
    Ok(apply_result(h, entries))
}

#[tauri::command]
#[specta::specta]
pub fn undo(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let entries = h.undo()?.unwrap_or_default();
    schedule_thumbnail(&app, &id);
    Ok(apply_result(&h, entries))
}

#[tauri::command]
#[specta::specta]
pub fn redo(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let entries = h.redo()?.unwrap_or_default();
    schedule_thumbnail(&app, &id);
    Ok(apply_result(&h, entries))
}

#[tauri::command]
#[specta::specta]
pub fn restore_version(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    seq: u64,
) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let entries = h.restore_version(seq)?;
    let _ = h.snapshot(
        Actor::User,
        &format!("{seq}번으로 되돌림 · 나"),
        Some(format!("restore:{seq}")),
        None,
    );
    schedule_thumbnail(&app, &id);
    Ok(apply_result(&h, entries))
}
