use banner_core::model::*;
use banner_core::store::ProjectHandle;
use serde::Serialize;
use tauri::State;

use super::project::refresh_thumbnail;
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
pub fn apply_ops(state: State<'_, AppState>, id: String, ops: Vec<Op>, actor: Actor) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let entries = h.apply_ops(ops, actor, None)?;
    refresh_thumbnail(&state, &mut h);
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
        let thumb = img::thumbnail_png(&h, &state.fonts).ok();
        let summary = format!(
            "직접 편집 · {}",
            entries
                .first()
                .map(|e| agent::describe_entry(e, &h.document))
                .unwrap_or_default()
        );
        let _ = h.snapshot(Actor::User, &summary, None, thumb.as_deref());
    }
    Ok(apply_result(&h, entries))
}

#[tauri::command]
#[specta::specta]
pub fn undo(state: State<'_, AppState>, id: String) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let entries = h.undo()?.unwrap_or_default();
    refresh_thumbnail(&state, &mut h);
    Ok(apply_result(&h, entries))
}

#[tauri::command]
#[specta::specta]
pub fn redo(state: State<'_, AppState>, id: String) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let entries = h.redo()?.unwrap_or_default();
    refresh_thumbnail(&state, &mut h);
    Ok(apply_result(&h, entries))
}

#[tauri::command]
#[specta::specta]
pub fn restore_version(state: State<'_, AppState>, id: String, seq: u64) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let entries = h.restore_version(seq)?;
    refresh_thumbnail(&state, &mut h);
    let thumb = img::thumbnail_png(&h, &state.fonts).ok();
    let _ = h.snapshot(
        Actor::User,
        &format!("{seq}번으로 되돌림 · 나"),
        Some(format!("restore:{seq}")),
        thumb.as_deref(),
    );
    Ok(apply_result(&h, entries))
}
