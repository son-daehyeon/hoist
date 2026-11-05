use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::agent::img;
use crate::error::Result;
use crate::state::AppState;

#[derive(Serialize, specta::Type)]
pub struct ImportedImage {
    asset: String,
    width_px: u32,
    height_px: u32,
}

#[tauri::command]
#[specta::specta]
pub fn import_image(state: State<'_, AppState>, id: String, path: String) -> Result<ImportedImage> {
    let h = state.handle(&id)?;
    let h = img::lock(&h);
    let (asset, width_px, height_px) = h.import_image(Path::new(&path))?;
    Ok(ImportedImage {
        asset,
        width_px,
        height_px,
    })
}
