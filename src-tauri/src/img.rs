use anyhow::Context;
use hoist_core::image as cimg;
use hoist_core::render::{RenderOptions, render_document};
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;
use image::RgbaImage;

pub const THUMBNAIL_PX: u32 = 640;

pub fn data_url(png: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

pub fn work_scale(handle: &ProjectHandle, max_px: u32) -> f64 {
    let w = handle.document.artboard.width_mm.max(1.0);
    let h = handle.document.artboard.height_mm.max(1.0);
    (max_px as f64 / w).min(max_px as f64 / h)
}

pub fn work_image(handle: &ProjectHandle, fonts: &Fonts, max_px: u32) -> anyhow::Result<RgbaImage> {
    let scale = work_scale(handle, max_px);
    Ok(render_document(
        &handle.document,
        &handle.assets_dir(),
        fonts,
        &RenderOptions {
            scale_px_per_mm: scale,
            background: true,
        },
    )?)
}

pub fn original_image(handle: &ProjectHandle, version: u32) -> anyhow::Result<RgbaImage> {
    let path = handle.original_path(version);
    cimg::load(&path).with_context(|| format!("원본 v{version}을 읽지 못했어요"))
}

pub fn thumbnail_png(handle: &ProjectHandle, fonts: &Fonts) -> anyhow::Result<Vec<u8>> {
    let img = work_image(handle, fonts, THUMBNAIL_PX)?;
    Ok(cimg::to_png_bytes(&img)?)
}
