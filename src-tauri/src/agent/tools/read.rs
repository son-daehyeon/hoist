use anyhow::bail;
use hoist_core::model::{Document, LayerKind};
use hoist_core::render::{RenderOptions, render_document};
use hoist_core::{analyze, fontmatch, image as cimg, trace};
use image::RgbaImage;
use serde_json::{Value, json};

use super::{
    ToolCtx, ToolResult, arg_rect_mm, arg_rect_px, arg_rect_px_opt, arg_str, r1, require_original, version_of,
};
use crate::agent::img;
use crate::state::lock;

pub(super) fn document_summary(doc: &Document) -> Value {
    let layers: Vec<Value> = doc
        .layers
        .iter()
        .map(|l| {
            let (kind, extra) = match &l.kind {
                LayerKind::Group => ("group", Value::Null),
                LayerKind::Text(t) => (
                    "text",
                    json!({"text": t.text, "font": t.font_family, "size_mm": t.font_size_mm}),
                ),
                LayerKind::Path(_) => ("path", Value::Null),
                LayerKind::Shape(s) => ("shape", serde_json::to_value(s).unwrap_or(Value::Null)),
                LayerKind::Image(i) => (
                    "image",
                    json!({"asset": i.asset, "px": [i.natural_w_px, i.natural_h_px]}),
                ),
            };
            json!({"id": l.id, "name": l.name, "parent": l.parent, "kind": kind, "extra": extra,
                "x_mm": r1(l.x_mm), "y_mm": r1(l.y_mm), "w_mm": r1(l.w_mm), "h_mm": r1(l.h_mm),
                "visible": l.visible, "strategy": l.strategy, "pinned": l.pinned,
                "fills": l.fills.len(), "strokes": l.strokes.len(), "effects": l.effects.len()})
        })
        .collect();
    json!({"rev": doc.rev, "artboard": doc.artboard, "layers": layers})
}

pub(super) fn view_original(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let version = version_of(ctx, args)?;
    let max_px = args["max_px"].as_u64().unwrap_or(1536).clamp(256, 3000) as u32;
    let h = lock(&ctx.handle);
    let orig = img::original_image(&h, version)?;
    let (img_out, rect) = match arg_rect_px_opt(args, "rect_px")? {
        Some(r) => (cimg::crop(&orig, r), Some(r)),
        None => (orig.clone(), None),
    };
    let small = cimg::resize_max(&img_out, max_px);
    Ok(ToolResult::with_image(
        json!({"version": version, "width_px": orig.width(), "height_px": orig.height(), "shown_rect_px": rect, "shown_px": [small.width(), small.height()]}),
        cimg::to_png_bytes(&small)?,
    ))
}

pub(super) fn view_work(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let max_px = args["max_px"].as_u64().unwrap_or(1536).clamp(256, 3000) as u32;
    let h = lock(&ctx.handle);
    let image = if let Some(ids) = args["layer_ids"].as_array() {
        let ids: Vec<String> = ids
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let mut doc = h.document.clone();
        for l in doc.layers.iter_mut() {
            let keep =
                ids.iter().any(|i| i == &l.id) || l.parent.as_ref().map(|p| ids.contains(p)).unwrap_or(false);
            if !keep && !matches!(l.kind, LayerKind::Group) {
                l.visible = false;
            }
        }
        let scale = img::work_scale(&h, max_px);
        render_document(
            &doc,
            &h.assets_dir(),
            &ctx.fonts,
            &RenderOptions {
                scale_px_per_mm: scale,
                background: false,
            },
        )?
    } else if let Some(r) = arg_rect_mm(args, "rect_mm") {
        img::work_region(
            &h.document,
            &h.assets_dir(),
            &ctx.fonts,
            img::clamp_rect_mm(&h, r),
            max_px,
        )?
    } else {
        img::work_image(&h, &ctx.fonts, max_px)?
    };
    Ok(ToolResult::with_image(
        json!({"shown_px": [image.width(), image.height()], "rev": h.document.rev}),
        cimg::to_png_bytes(&image)?,
    ))
}

pub(super) fn font_lineup(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let text = arg_str(args, "text")?;
    let families: Vec<String> = args["families"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if families.is_empty() || families.len() > 8 {
        bail!("families must hold 1–8 family names from list_fonts");
    }
    let missing: Vec<&str> = families
        .iter()
        .filter(|f| !ctx.fonts.has_family(f))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        bail!(
            "unknown families: {} — use family names from list_fonts",
            missing.join(", ")
        );
    }
    let weight = args["weight"].as_u64().unwrap_or(700).clamp(100, 900) as u16;
    let lineup = fontmatch::font_lineup(&ctx.fonts, &text, &families, weight, 20.0)?;
    let (image, body) = match arg_rect_px_opt(args, "rect_px")? {
        Some(rect) => {
            let version = version_of(ctx, args)?;
            let original = img::original_image(&lock(&ctx.handle), version)?;
            let crop = cimg::crop(&original, rect);
            let crop_h = (f64::from(crop.height()) * f64::from(lineup.width())
                / f64::from(crop.width().max(1)))
            .round()
            .max(1.0) as u32;
            let fit = fontmatch::fit_fonts(&ctx.fonts, &original, rect, &text, &families, weight)?;
            (
                stack_vertical(&cimg::resize_to(&crop, lineup.width(), crop_h), &lineup, 24),
                json!({"families": families, "weight": weight, "matches": fit.matches, "reliable": fit.reliable, "note": "Top: the original crop. Below: each candidate labelled with its family. matches lists, per family, the overlap score with the original's ink and the scale_x / letter_spacing_em that reproduce its proportions (fitted on the first line of the crop). When reliable is true use the top score; ties within 0.02 are broken by stroke ends in the picture. When reliable is false judge by the picture and still use the fitted scale_x of the family you pick."}),
            )
        }
        None => (
            lineup,
            json!({"families": families, "weight": weight, "note": "Top: the original crop (when rect_px was given). Below: each candidate labelled with its family. Compare the skeletons (ㅇ roundness, stroke ends, ㄹ/ㅁ proportions, baseline), pick the closest, then tune font_weight, scale_x and letter_spacing_mm on the text layer."}),
        ),
    };
    Ok(ToolResult::with_image(
        body,
        cimg::to_png_bytes(&cimg::resize_max(&image, 1536))?,
    ))
}

fn stack_vertical(top: &RgbaImage, bottom: &RgbaImage, gap: u32) -> RgbaImage {
    let w = top.width().max(bottom.width());
    let mut out = RgbaImage::from_pixel(
        w,
        top.height() + gap + bottom.height(),
        image::Rgba([255, 255, 255, 255]),
    );
    for (src, dy) in [(top, 0), (bottom, top.height() + gap)] {
        for (x, y, p) in src.enumerate_pixels() {
            out.put_pixel(x, y + dy, *p);
        }
    }
    out
}

pub(super) fn compare_tool(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let f = require_original(ctx)?;
    let h = lock(&ctx.handle);
    if let Some(r) = arg_rect_mm(args, "rect_mm") {
        let rect = img::clamp_rect_mm(&h, r);
        let original = img::original_image(&h, f.version)?;
        let (res, png) =
            img::compare_region(&h.document, &h.assets_dir(), &original, &ctx.fonts, f, rect, 1200)?;
        return Ok(ToolResult::with_image(
            json!({"region_mm": rect, "result": res, "image": "왼쪽 원본 · 오른쪽 작업본"}),
            png,
        ));
    }
    let full = img::compare_full(&h, &ctx.fonts, f, 1200)?;
    Ok(ToolResult::with_image(
        json!({"result": full.result, "image": "왼쪽 원본 · 오른쪽 작업본"}),
        full.side_by_side_png,
    ))
}

pub(super) fn analyze_original(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let version = version_of(ctx, args)?;
    let f = require_original(ctx)?;
    let h = lock(&ctx.handle);
    let cache = h.dir.join("analysis").join(format!("v{version}.json"));
    let cached: Option<Value> = std::fs::read(&cache)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let analysis = match cached {
        Some(v) => v,
        None => {
            let orig = img::original_image(&h, version)?;
            let small = cimg::resize_max(&orig, 1200);
            let scale = orig.width() as f64 / small.width() as f64;
            let pal = analyze::palette(&small, 10);
            let regions: Vec<Value> = analyze::segment_regions(&small)
                .into_iter()
                .map(|r| {
                    let b = r.bbox_px;
                    json!({"bbox_px": [(b[0] as f64 * scale).round(), (b[1] as f64 * scale).round(), (b[2] as f64 * scale).round(), (b[3] as f64 * scale).round()], "color": r.color, "kind": r.kind_hint, "area_px": (r.area_px as f64 * scale * scale).round()})
                })
                .collect();
            let v = json!({"width_px": orig.width(), "height_px": orig.height(), "palette": pal.colors, "regions": regions});
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&cache, serde_json::to_vec(&v)?);
            v
        }
    };
    Ok(ToolResult::json(
        json!({"version": version, "width_px": analysis["width_px"], "height_px": analysis["height_px"], "mm_per_px": f.mm_per_px, "offset_mm": [f.offset_x_mm, f.offset_y_mm], "palette": analysis["palette"], "regions": analysis["regions"]}),
    ))
}

pub(super) fn measure(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let version = version_of(ctx, args)?;
    let rect = arg_rect_px(args, "rect_px")?;
    let h = lock(&ctx.handle);
    let orig = img::original_image(&h, version)?;
    let crop = cimg::crop(&orig, rect);
    let pal = analyze::palette(&crop, 6);
    let dominant = cimg::dominant_color(&crop, None);
    let mut out = json!({"rect_px": rect, "dominant": dominant, "palette": pal.colors});
    if let Some(c) = args["color"].as_str() {
        let tol = args["tolerance"].as_u64().unwrap_or(40) as u8;
        let mask = trace::color_mask(&crop, c, tol);
        let bbox = mask_bbox(&mask).map(|b| [b[0] + rect[0], b[1] + rect[1], b[2], b[3]]);
        out["color_bbox_px"] = json!(bbox);
    }
    Ok(ToolResult::json(out))
}

fn mask_bbox(mask: &image::GrayImage) -> Option<[u32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for (x, y, p) in mask.enumerate_pixels() {
        if p[0] > 127 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 == u32::MAX {
        None
    } else {
        Some([x0, y0, x1 - x0 + 1, y1 - y0 + 1])
    }
}
