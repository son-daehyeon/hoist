use hoist_core::boolean::{self, BooleanOp};
use hoist_core::model::*;
use tauri::{AppHandle, State};

use super::document::{ApplyResult, apply_and_record};
use crate::agent::img;
use crate::error::Result;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn path_boolean(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    layer_ids: Vec<LayerId>,
    op: BooleanOp,
) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let ops = boolean::path_boolean(&h.document, &state.fonts, &layer_ids, op)?;
    apply_and_record(&app, &id, &mut h, ops, Actor::User)
}

#[tauri::command]
#[specta::specta]
pub fn outline_text(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    layer_id: LayerId,
) -> Result<ApplyResult> {
    let h = state.handle(&id)?;
    let mut h = img::lock(&h);
    let ops = boolean::outline_text(&h.document, &state.fonts, &layer_id)?;
    apply_and_record(&app, &id, &mut h, ops, Actor::User)
}
