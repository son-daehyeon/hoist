use std::path::Path;

use hoist_core::image as cimg;
use hoist_core::matting;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::{Error, Result};
use crate::state::{AppState, lock, models_dir};

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
    let h = lock(&h);
    let (asset, width_px, height_px) = h.import_image(Path::new(&path))?;
    Ok(ImportedImage {
        asset,
        width_px,
        height_px,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn remove_background(app: AppHandle, id: String, asset: String) -> Result<ImportedImage> {
    let models = models_dir(&app);
    if !matting::available(&models) {
        return Err(Error::Invalid(
            "누끼 모델이 없어요. Hoist를 다시 설치해 주세요".into(),
        ));
    }
    if Path::new(&asset).file_name().and_then(|n| n.to_str()) != Some(asset.as_str()) {
        return Err(Error::Invalid(format!("그림 이름이 올바르지 않아요: {asset}")));
    }
    tokio::task::spawn_blocking(move || {
        let handle = app.state::<AppState>().handle(&id)?;
        let src = lock(&handle).asset_path(&asset);
        if !src.is_file() {
            return Err(hoist_core::CoreError::AssetNotFound(asset).into());
        }
        let cut = matting::remove_background(&models, &cimg::load(&src)?)?;
        let h = lock(&handle);
        let name = h.new_asset_name("nobg", "png");
        cimg::save_png(&cut, &h.asset_path(&name))?;
        Ok(ImportedImage {
            asset: name,
            width_px: cut.width(),
            height_px: cut.height(),
        })
    })
    .await
    .map_err(|e| Error::Other(e.into()))?
}
