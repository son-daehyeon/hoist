use hoist_core::image as cimg;
use hoist_core::render::{RenderOptions, render_document, render_layers as core_render_layers};
use serde::Serialize;
use tauri::State;

use crate::agent::img;
use crate::error::Result;
use crate::state::AppState;

#[derive(Serialize, specta::Type)]
pub struct LayerRasterView {
    id: String,
    png_base64: String,
    x_px: i32,
    y_px: i32,
    w_px: u32,
    h_px: u32,
}

#[derive(Serialize, specta::Type)]
pub struct RenderResultView {
    scale_px_per_mm: f64,
    width_px: u32,
    height_px: u32,
    layers: Vec<LayerRasterView>,
}

#[tauri::command]
#[specta::specta]
pub fn render_layers(
    state: State<'_, AppState>,
    id: String,
    scale_px_per_mm: f64,
) -> Result<RenderResultView> {
    let h = state.handle(&id)?;
    let h = img::lock(&h);
    let scale = scale_px_per_mm.clamp(0.01, 40.0);
    let rasters = core_render_layers(&h.document, &h.assets_dir(), &state.fonts, scale)?;
    let mut layers = Vec::new();
    let mut bg_doc = h.document.clone();
    bg_doc.layers.clear();
    let bg = render_document(
        &bg_doc,
        &h.assets_dir(),
        &state.fonts,
        &RenderOptions {
            scale_px_per_mm: scale,
            background: true,
        },
    )?;
    layers.push(LayerRasterView {
        id: "__background".into(),
        png_base64: cimg::to_base64_png(&bg)?,
        x_px: 0,
        y_px: 0,
        w_px: bg.width(),
        h_px: bg.height(),
    });
    for r in rasters {
        let png = match &r.image {
            Some(im) => cimg::to_base64_png(im)?,
            None => continue,
        };
        layers.push(LayerRasterView {
            id: r.id,
            png_base64: png,
            x_px: r.x_px,
            y_px: r.y_px,
            w_px: r.w_px,
            h_px: r.h_px,
        });
    }
    Ok(RenderResultView {
        scale_px_per_mm: scale,
        width_px: (h.document.artboard.width_mm * scale).round() as u32,
        height_px: (h.document.artboard.height_mm * scale).round() as u32,
        layers,
    })
}

#[tauri::command]
#[specta::specta]
pub fn render_original(state: State<'_, AppState>, id: String, version: u32, max_px: u32) -> Result<String> {
    let h = state.handle(&id)?;
    let h = img::lock(&h);
    let orig = img::original_image(&h, version)?;
    let small = cimg::resize_max(&orig, max_px.clamp(64, 4096));
    Ok(img::data_url(&cimg::to_png_bytes(&small)?))
}

#[tauri::command]
#[specta::specta]
pub fn render_composite(state: State<'_, AppState>, id: String, max_px: u32) -> Result<String> {
    let h = state.handle(&id)?;
    let h = img::lock(&h);
    let im = img::work_image(&h, &state.fonts, max_px.clamp(64, 4096))?;
    Ok(img::data_url(&cimg::to_png_bytes(&im)?))
}
