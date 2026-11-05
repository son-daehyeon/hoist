use std::path::Path;

use hoist_core::image as cimg;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::agent::img;
use crate::error::Result;
use crate::state::AppState;

#[derive(Serialize, specta::Type)]
pub struct ImageInfoView {
    width_px: u32,
    height_px: u32,
    preview_png: String,
}

#[tauri::command]
#[specta::specta]
pub async fn pick_image_file(app: AppHandle) -> Result<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .add_filter(
            "이미지",
            &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"],
        )
        .set_title("현수막 이미지 고르기")
        .blocking_pick_file();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
#[specta::specta]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("프로젝트 저장 폴더")
        .blocking_pick_folder();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
#[specta::specta]
pub async fn pick_save_path(app: AppHandle, default_name: String) -> Result<Option<String>> {
    let ext = default_name.rsplit('.').next().unwrap_or("png").to_string();
    let mut b = app
        .dialog()
        .file()
        .set_file_name(&default_name)
        .set_title("내보내기");
    if let Ok(d) = app.path().desktop_dir() {
        b = b.set_directory(d);
    }
    let picked = b
        .add_filter(ext.to_uppercase(), &[ext.as_str()])
        .blocking_save_file();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
#[specta::specta]
pub fn show_in_folder(state: State<'_, AppState>, id: String) -> Result<()> {
    let dir = state.store().root().join(&id);
    Ok(tauri_plugin_opener::reveal_item_in_dir(&dir)?)
}

#[tauri::command]
#[specta::specta]
pub fn read_image_info(path: String) -> Result<ImageInfoView> {
    let im = cimg::load(Path::new(&path))?;
    let preview = cimg::to_png_bytes(&cimg::resize_max(&im, 640))?;
    Ok(ImageInfoView {
        width_px: im.width(),
        height_px: im.height(),
        preview_png: img::data_url(&preview),
    })
}
