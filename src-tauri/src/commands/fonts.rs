use std::collections::BTreeSet;

use hoist_core::text::FontStyle;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::Result;
use crate::state::AppState;

#[derive(Serialize, specta::Type)]
pub struct FontFileView {
    weight: u16,
    style: FontStyle,
    path: String,
}

#[derive(Serialize, specta::Type)]
pub struct FontView {
    family: String,
    display_name: Option<String>,
    category: Option<String>,
    bundled: bool,
    weights: Vec<u16>,
    files: Vec<FontFileView>,
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_fonts(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<FontView>> {
    let catalog = state.fonts.catalog();
    let dirs: BTreeSet<_> = catalog
        .iter()
        .flat_map(|f| &f.faces)
        .filter_map(|face| face.path.parent().map(|d| d.to_path_buf()))
        .collect();
    let scope = app.asset_protocol_scope();
    for dir in dirs {
        scope.allow_directory(&dir, false).map_err(anyhow::Error::from)?;
    }
    Ok(catalog
        .into_iter()
        .map(|f| FontView {
            files: f
                .faces
                .iter()
                .flat_map(|face| {
                    let path = face.path.to_string_lossy().into_owned();
                    face.weights.iter().map(move |&weight| FontFileView {
                        weight,
                        style: face.style,
                        path: path.clone(),
                    })
                })
                .collect(),
            family: f.family,
            display_name: f.display_name,
            category: f.category,
            bundled: f.bundled,
            weights: f.weights,
        })
        .collect())
}
