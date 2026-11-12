use std::collections::HashSet;
use std::time::Instant;

use hoist_core::image as cimg;
use serde::Serialize;
use tauri::State;

use crate::error::Result;
use crate::img;
use crate::state::{AppState, lock};

#[derive(Serialize, specta::Type)]
pub struct LayerRasterView {
    id: String,
    key: String,
    png_base64: Option<String>,
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

#[tauri::command(async)]
#[specta::specta]
pub fn render_layers(
    state: State<'_, AppState>,
    id: String,
    scale_px_per_mm: f64,
    have: Vec<String>,
) -> Result<RenderResultView> {
    let started = Instant::now();
    let have: HashSet<String> = have.into_iter().collect();
    let handle = state.handle(&id)?;
    let mut guard = lock(&handle);
    let scale = scale_px_per_mm.clamp(0.01, 40.0);
    let assets = guard.assets_dir();
    let h = &mut *guard;
    let background = h
        .render_cache
        .background(&h.document, &assets, &state.fonts, scale)?;
    let (rasters, hits) = h.render_cache.layers(&h.document, &assets, &state.fonts, scale)?;
    let total = rasters.len();
    let mut sent = 0;
    let layers = std::iter::once(background)
        .chain(rasters)
        .map(|r| {
            let key = format!("{:016x}", r.key);
            let png_base64 = (!have.contains(&key)).then_some(r.png_base64);
            sent += usize::from(png_base64.is_some());
            LayerRasterView {
                id: r.id,
                key,
                png_base64,
                x_px: r.x_px,
                y_px: r.y_px,
                w_px: r.w_px,
                h_px: r.h_px,
            }
        })
        .collect();
    log::debug!(
        "render_layers {id} scale {scale}: {total} layers, {hits} cached, {sent} sent, {} ms",
        started.elapsed().as_millis()
    );
    Ok(RenderResultView {
        scale_px_per_mm: scale,
        width_px: (h.document.artboard.width_mm * scale).round() as u32,
        height_px: (h.document.artboard.height_mm * scale).round() as u32,
        layers,
    })
}

#[derive(Serialize, specta::Type)]
pub struct LayerSvgView {
    id: String,
    key: String,
    svg: Option<String>,
    bbox_mm: [f64; 4],
    inks: Vec<TextInkView>,
}

#[derive(Serialize, specta::Type)]
pub struct TextInkView {
    id: String,
    frac: [f64; 4],
}

#[derive(Serialize, specta::Type)]
pub struct LayerSvgsView {
    width_mm: f64,
    height_mm: f64,
    assets_dir: String,
    layers: Vec<LayerSvgView>,
}

#[tauri::command(async)]
#[specta::specta]
pub fn layer_svgs(state: State<'_, AppState>, id: String, have: Vec<String>) -> Result<LayerSvgsView> {
    let started = Instant::now();
    let have: HashSet<String> = have.into_iter().collect();
    let handle = state.handle(&id)?;
    let mut guard = lock(&handle);
    let assets_dir = guard.assets_dir().to_string_lossy().into_owned();
    let h = &mut *guard;
    let (svgs, hits) = h.render_cache.layer_svgs(&h.document, &state.fonts)?;
    let total = svgs.len();
    let layers: Vec<LayerSvgView> = svgs
        .into_iter()
        .map(|s| {
            let key = format!("{:016x}", s.key);
            LayerSvgView {
                svg: (!have.contains(&key)).then_some(s.svg),
                id: s.id,
                key,
                bbox_mm: s.bbox_mm,
                inks: s
                    .inks
                    .into_iter()
                    .map(|t| TextInkView {
                        id: t.id,
                        frac: t.frac,
                    })
                    .collect(),
            }
        })
        .collect();
    let sent = layers.iter().filter(|l| l.svg.is_some()).count();
    log::debug!(
        "layer_svgs {id}: {total} layers, {hits} cached, {sent} sent, {} ms",
        started.elapsed().as_millis()
    );
    Ok(LayerSvgsView {
        width_mm: h.document.artboard.width_mm,
        height_mm: h.document.artboard.height_mm,
        assets_dir,
        layers,
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn render_original(state: State<'_, AppState>, id: String, version: u32, max_px: u32) -> Result<String> {
    let h = state.handle(&id)?;
    let h = lock(&h);
    let orig = img::original_image(&h, version)?;
    let small = cimg::resize_max(&orig, max_px.clamp(64, 4096));
    Ok(img::data_url(&cimg::to_png_bytes(&small)?))
}

#[tauri::command(async)]
#[specta::specta]
pub fn render_composite(state: State<'_, AppState>, id: String, max_px: u32) -> Result<String> {
    let h = state.handle(&id)?;
    let h = lock(&h);
    let im = img::work_image(&h, &state.fonts, max_px.clamp(64, 4096))?;
    Ok(img::data_url(&cimg::to_png_bytes(&im)?))
}
