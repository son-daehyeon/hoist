use std::sync::{Arc, Mutex};

use anyhow::Context;
use banner_core::render::{RenderOptions, render_document, render_region};
use banner_core::store::ProjectHandle;
use banner_core::text::Fonts;
use banner_core::{compare, image as cimg};
use image::{Rgba, RgbaImage};

use super::prompt::OriginalFacts;

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

pub fn work_region(
    handle: &ProjectHandle,
    fonts: &Fonts,
    rect_mm: [f64; 4],
    max_px: u32,
) -> anyhow::Result<RgbaImage> {
    let scale = (max_px as f64 / rect_mm[2].max(1.0)).min(max_px as f64 / rect_mm[3].max(1.0));
    Ok(render_region(
        &handle.document,
        &handle.assets_dir(),
        fonts,
        scale,
        rect_mm,
    )?)
}

pub fn original_image(handle: &ProjectHandle, version: u32) -> anyhow::Result<RgbaImage> {
    let path = handle.original_path(version);
    cimg::load(&path).with_context(|| format!("원본 v{version}을 읽지 못했어요"))
}

pub fn original_fitted(
    handle: &ProjectHandle,
    facts: &OriginalFacts,
    width_px: u32,
    height_px: u32,
) -> anyhow::Result<RgbaImage> {
    let orig = original_image(handle, facts.version)?;
    let px_per_mm = width_px as f64 / handle.document.artboard.width_mm.max(1.0);
    let ow = ((orig.width() as f64 * facts.mm_per_px) * px_per_mm)
        .round()
        .max(1.0) as u32;
    let oh = ((orig.height() as f64 * facts.mm_per_px) * px_per_mm)
        .round()
        .max(1.0) as u32;
    let scaled = cimg::resize_to(&orig, ow, oh);
    let fill = edge_color(&orig);
    let mut canvas = RgbaImage::from_pixel(width_px, height_px, fill);
    let ox = (facts.offset_x_mm * px_per_mm).round() as i64;
    let oy = (facts.offset_y_mm * px_per_mm).round() as i64;
    for (x, y, p) in scaled.enumerate_pixels() {
        let cx = x as i64 + ox;
        let cy = y as i64 + oy;
        if cx >= 0 && cy >= 0 && (cx as u32) < width_px && (cy as u32) < height_px {
            canvas.put_pixel(cx as u32, cy as u32, *p);
        }
    }
    Ok(canvas)
}

fn edge_color(img: &RgbaImage) -> Rgba<u8> {
    let (w, h) = (img.width(), img.height());
    let mut acc = [0u64; 3];
    let mut n = 0u64;
    let mut add = |p: &Rgba<u8>| {
        acc[0] += p[0] as u64;
        acc[1] += p[1] as u64;
        acc[2] += p[2] as u64;
        n += 1;
    };
    for x in 0..w {
        add(img.get_pixel(x, 0));
        add(img.get_pixel(x, h - 1));
    }
    for y in 0..h {
        add(img.get_pixel(0, y));
        add(img.get_pixel(w - 1, y));
    }
    if n == 0 {
        return Rgba([255, 255, 255, 255]);
    }
    Rgba([(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, 255])
}

pub struct FullCompare {
    pub result: compare::CompareResult,
    pub side_by_side_png: Vec<u8>,
    pub original_png: Vec<u8>,
    pub work_png: Vec<u8>,
}

pub fn compare_full(
    handle: &ProjectHandle,
    fonts: &Fonts,
    facts: &OriginalFacts,
    max_px: u32,
) -> anyhow::Result<FullCompare> {
    let work = work_image(handle, fonts, max_px)?;
    let orig = original_fitted(handle, facts, work.width(), work.height())?;
    let result = compare::compare(&orig, &work);
    let sbs = cimg::side_by_side(&orig, &work, 16);
    Ok(FullCompare {
        result,
        side_by_side_png: cimg::to_png_bytes(&sbs)?,
        original_png: cimg::to_png_bytes(&cimg::resize_max(&orig, 600))?,
        work_png: cimg::to_png_bytes(&cimg::resize_max(&work, 600))?,
    })
}

pub fn compare_region(
    handle: &ProjectHandle,
    fonts: &Fonts,
    facts: &OriginalFacts,
    rect_mm: [f64; 4],
    max_px: u32,
) -> anyhow::Result<(compare::CompareResult, Vec<u8>)> {
    let work = work_region(handle, fonts, rect_mm, max_px)?;
    let full_w = (handle.document.artboard.width_mm * work.width() as f64 / rect_mm[2].max(1.0))
        .round()
        .max(1.0) as u32;
    let full_h = (handle.document.artboard.height_mm * work.height() as f64 / rect_mm[3].max(1.0))
        .round()
        .max(1.0) as u32;
    let orig_full = original_fitted(handle, facts, full_w, full_h)?;
    let px_per_mm = full_w as f64 / handle.document.artboard.width_mm.max(1.0);
    let rect_px = [
        (rect_mm[0] * px_per_mm).max(0.0) as u32,
        (rect_mm[1] * px_per_mm).max(0.0) as u32,
        (rect_mm[2] * px_per_mm).max(1.0) as u32,
        (rect_mm[3] * px_per_mm).max(1.0) as u32,
    ];
    let orig = cimg::crop(&orig_full, rect_px);
    let result = compare::compare(&orig, &work);
    let sbs = cimg::side_by_side(&orig, &work, 12);
    Ok((result, cimg::to_png_bytes(&sbs)?))
}

pub fn clamp_rect_mm(handle: &ProjectHandle, r: [f64; 4]) -> [f64; 4] {
    let w = handle.document.artboard.width_mm;
    let h = handle.document.artboard.height_mm;
    let x = r[0].clamp(0.0, w);
    let y = r[1].clamp(0.0, h);
    let rw = r[2].max(1.0).min(w - x);
    let rh = r[3].max(1.0).min(h - y);
    [x, y, rw, rh]
}

pub fn thumbnail_png(handle: &ProjectHandle, fonts: &Fonts) -> anyhow::Result<Vec<u8>> {
    let img = work_image(handle, fonts, 640)?;
    Ok(cimg::to_png_bytes(&img)?)
}

pub fn lock<'a>(h: &'a Arc<Mutex<ProjectHandle>>) -> std::sync::MutexGuard<'a, ProjectHandle> {
    h.lock().unwrap_or_else(|e| e.into_inner())
}
