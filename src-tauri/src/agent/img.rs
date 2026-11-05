use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use hoist_core::model::Document;
use hoist_core::render::{RenderOptions, render_document, render_region};
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;
use hoist_core::{compare, image as cimg};
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
    doc: &Document,
    assets_dir: &Path,
    fonts: &Fonts,
    rect_mm: [f64; 4],
    max_px: u32,
) -> anyhow::Result<RgbaImage> {
    let scale = (max_px as f64 / rect_mm[2].max(1.0)).min(max_px as f64 / rect_mm[3].max(1.0));
    Ok(render_region(doc, assets_dir, fonts, scale, rect_mm)?)
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
    let ab = &handle.document.artboard;
    Ok(original_in_rect(
        &orig,
        facts,
        [0.0, 0.0, ab.width_mm, ab.height_mm],
        width_px,
        height_px,
    ))
}

/// The original as placed on the artboard, seen through `rect_mm` at `width_px`×`height_px`.
/// Only the overlapping part of the original is resized, so small regions stay cheap.
pub fn original_in_rect(
    orig: &RgbaImage,
    facts: &OriginalFacts,
    rect_mm: [f64; 4],
    width_px: u32,
    height_px: u32,
) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(width_px, height_px, edge_color(orig));
    let mm = facts.mm_per_px.max(1e-9);
    let sx = f64::from(width_px) / rect_mm[2].max(1e-9);
    let sy = f64::from(height_px) / rect_mm[3].max(1e-9);
    let px = |v: f64, off: f64, max: u32| ((v - off) / mm).clamp(0.0, f64::from(max));
    let x0 = px(rect_mm[0], facts.offset_x_mm, orig.width()).floor();
    let y0 = px(rect_mm[1], facts.offset_y_mm, orig.height()).floor();
    let x1 = px(rect_mm[0] + rect_mm[2], facts.offset_x_mm, orig.width()).ceil();
    let y1 = px(rect_mm[1] + rect_mm[3], facts.offset_y_mm, orig.height()).ceil();
    if x1 <= x0 || y1 <= y0 {
        return canvas;
    }
    let crop = cimg::crop(orig, [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32]);
    let dw = ((x1 - x0) * mm * sx).round().max(1.0) as u32;
    let dh = ((y1 - y0) * mm * sy).round().max(1.0) as u32;
    let scaled = cimg::resize_to(&crop, dw, dh);
    let dx = ((facts.offset_x_mm + x0 * mm - rect_mm[0]) * sx).round() as i64;
    let dy = ((facts.offset_y_mm + y0 * mm - rect_mm[1]) * sy).round() as i64;
    for (x, y, p) in scaled.enumerate_pixels() {
        let cx = x as i64 + dx;
        let cy = y as i64 + dy;
        if cx >= 0 && cy >= 0 && (cx as u32) < width_px && (cy as u32) < height_px {
            canvas.put_pixel(cx as u32, cy as u32, *p);
        }
    }
    canvas
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

#[allow(clippy::too_many_arguments)]
pub fn compare_region(
    doc: &Document,
    assets_dir: &Path,
    original: &RgbaImage,
    fonts: &Fonts,
    facts: &OriginalFacts,
    rect_mm: [f64; 4],
    max_px: u32,
) -> anyhow::Result<(compare::CompareResult, Vec<u8>)> {
    let work = work_region(doc, assets_dir, fonts, rect_mm, max_px)?;
    let orig = original_in_rect(original, facts, rect_mm, work.width(), work.height());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_of_the_original_matches_the_same_part_of_the_full_fit() {
        let orig = RgbaImage::from_fn(200, 100, |x, y| {
            Rgba([((x + y) / 2) as u8, x as u8, 255 - y as u8, 255])
        });
        let facts = OriginalFacts {
            version: 1,
            width_px: 200,
            height_px: 100,
            mm_per_px: 2.0,
            offset_x_mm: 30.0,
            offset_y_mm: 10.0,
        };
        let full = original_in_rect(&orig, &facts, [0.0, 0.0, 460.0, 220.0], 920, 440);
        let region = original_in_rect(&orig, &facts, [100.0, 50.0, 200.0, 100.0], 400, 200);
        let same_part = cimg::crop(&full, [200, 100, 400, 200]);
        let total: u64 = region
            .pixels()
            .zip(same_part.pixels())
            .flat_map(|(a, b)| a.0.iter().zip(b.0.iter()).map(|(x, y)| u64::from(x.abs_diff(*y))))
            .sum();
        let mean = total as f64 / (400.0 * 200.0 * 4.0);
        assert!(mean < 2.0, "mean channel difference {mean}");

        let outside = original_in_rect(&orig, &facts, [0.0, 0.0, 20.0, 8.0], 40, 16);
        let fill = *outside.get_pixel(0, 0);
        assert!(
            outside.pixels().all(|p| *p == fill),
            "left of and above the original is the edge fill"
        );
    }
}
