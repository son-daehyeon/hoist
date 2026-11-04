use std::path::PathBuf;

use banner_core::check::{ExportWarning, export_check as core_export_check};
use banner_core::export::{export_png, export_svg};
use banner_core::store::ProjectHandle;
use serde::{Deserialize, Deserializer, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::agent::img;
use crate::error::Result;
use crate::state::AppState;

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Png,
    Svg,
}

#[derive(Clone, Copy, PartialEq, Eq, specta::Type)]
pub enum ExportScale {
    One,
    Ten,
}

impl<'de> Deserialize<'de> for ExportScale {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Ok(match u32::deserialize(deserializer)? {
            10 => ExportScale::Ten,
            _ => ExportScale::One,
        })
    }
}

#[derive(Deserialize, specta::Type)]
pub struct ExportOptionsIn {
    format: ExportFormat,
    scale: ExportScale,
    dpi: f64,
    #[serde(default)]
    outline_text: bool,
}

#[derive(Serialize, specta::Type)]
pub struct ExportResultView {
    path: String,
    width_px: Option<u32>,
    height_px: Option<u32>,
    bytes: u64,
    warnings: Vec<ExportWarning>,
}

fn default_export_path(app: &AppHandle, h: &ProjectHandle, scale: u32, ext: &str) -> PathBuf {
    let dir = app.path().desktop_dir().unwrap_or_else(|_| h.dir.clone());
    let safe: String = h
        .project
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{}_1to{}.{}", safe, scale, ext))
}

#[tauri::command]
#[specta::specta]
pub fn export_project(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    options: ExportOptionsIn,
    dest_path: Option<String>,
) -> Result<ExportResultView> {
    let h = state.handle(&id)?;
    let h = img::lock(&h);
    let scale = match options.scale {
        ExportScale::One => 1,
        ExportScale::Ten => 10,
    };
    let ext = match options.format {
        ExportFormat::Png => "png",
        ExportFormat::Svg => "svg",
    };
    let out = dest_path
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_export_path(&app, &h, scale, ext));
    let warnings = core_export_check(&h.document, options.dpi);
    let info = if options.format == ExportFormat::Svg {
        export_svg(
            &h.document,
            &h.assets_dir(),
            &state.fonts,
            scale,
            options.outline_text,
            &out,
        )
    } else {
        export_png(
            &h.document,
            &h.assets_dir(),
            &state.fonts,
            options.dpi,
            scale,
            &out,
        )
    }?;
    Ok(ExportResultView {
        path: info.path,
        width_px: info.width_px,
        height_px: info.height_px,
        bytes: info.bytes,
        warnings,
    })
}

#[tauri::command]
#[specta::specta]
pub fn export_check(state: State<'_, AppState>, id: String, dpi: f64) -> Result<Vec<ExportWarning>> {
    let h = state.handle(&id)?;
    let h = img::lock(&h);
    Ok(core_export_check(&h.document, dpi))
}
