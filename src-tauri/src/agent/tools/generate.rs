use anyhow::{anyhow, bail};
use hoist_core::image as cimg;
use hoist_core::store::ProjectHandle;
use image::RgbaImage;
use serde_json::{Value, json};

use super::outpaint::{outpaint, snap16, trim_bars};
use super::turn::plan_quality;
use super::{ToolCtx, ToolResult, arg_str, asset_stem};
use crate::agent::img;
use crate::agent::prompt::OriginalFacts;
use crate::openai::{ImageOut, ImageRequest, image_cost_krw};
use crate::state::lock;

const CLEAN_SUFFIX: &str = "clean, no noise, no dust, no artifacts";
/// The model letterboxes a reference whose aspect does not fit the requested size.
const FILL_SUFFIX: &str =
    "Fill the whole canvas edge to edge with the scene; no borders, frames or letterbox bars";

fn clean_prompt(prompt: &str) -> String {
    format!("{}. {CLEAN_SUFFIX}", prompt.trim_end().trim_end_matches('.'))
}

const MAX_GEN_PX: u32 = 8_000_000;

fn clamp_gen_size(w: u32, h: u32) -> (u32, u32) {
    let mut w = w as f64;
    let mut h = h as f64;
    let ratio = w / h;
    if ratio > 3.0 {
        h = w / 3.0;
    } else if ratio < 1.0 / 3.0 {
        w = h / 3.0;
    }
    let long = w.max(h);
    if long > 3840.0 {
        let s = 3840.0 / long;
        w *= s;
        h *= s;
    }
    let px = w * h;
    if px > f64::from(MAX_GEN_PX) {
        let s = (f64::from(MAX_GEN_PX) / px).sqrt();
        w *= s;
        h *= s;
    }
    if px < 655_360.0 {
        let s = (655_360.0 / px).sqrt();
        w *= s;
        h *= s;
    }
    let (mut w, mut h) = (snap16(w.round() as u32), snap16(h.round() as u32));
    while w * h > MAX_GEN_PX {
        if w >= h {
            w -= 16;
        } else {
            h -= 16;
        }
    }
    (w, h)
}

/// The image API rejects some sizes we compute as over its pixel budget; one retry at half the area.
async fn request_image(ctx: &ToolCtx, req: ImageRequest) -> anyhow::Result<ImageOut> {
    let out = match ctx.openai.image(req.clone()).await {
        Err(e) if e.to_string().contains("pixel budget") && req.size.is_some() => {
            let (w, h) = req.size.unwrap_or_default();
            let s = std::f64::consts::FRAC_1_SQRT_2;
            let size = clamp_gen_size((f64::from(w) * s) as u32, (f64::from(h) * s) as u32);
            log::warn!(
                "image size {w}x{h} over the pixel budget, retrying at {}x{}",
                size.0,
                size.1
            );
            ctx.openai
                .image(ImageRequest {
                    size: Some(size),
                    ..req
                })
                .await
        }
        other => other,
    }?;
    *lock(&ctx.state.cost_krw) += image_cost_krw(&out.usage);
    Ok(out)
}

fn resolve_reference(h: &ProjectHandle, facts: Option<&OriginalFacts>, r: &str) -> anyhow::Result<Vec<u8>> {
    if r == "original" {
        let v = facts.ok_or_else(|| anyhow!("원본 없음"))?.version;
        let im = img::original_image(h, v)?;
        return Ok(cimg::to_png_bytes(&im)?);
    }
    if let Some(rest) = r.strip_prefix("original:") {
        let v = facts.ok_or_else(|| anyhow!("원본 없음"))?.version;
        let nums: Vec<u32> = rest.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if nums.len() != 4 {
            bail!("original:x,y,w,h 형식이어야 합니다");
        }
        let im = img::original_image(h, v)?;
        return Ok(cimg::to_png_bytes(&cimg::crop(
            &im,
            [nums[0], nums[1], nums[2], nums[3]],
        ))?);
    }
    let path = h.asset_path(r);
    if !path.exists() {
        bail!("asset {r} 없음");
    }
    Ok(std::fs::read(path)?)
}

fn repaint_mask(w: u32, h: u32, rects: &[Value]) -> RgbaImage {
    let mut mask = RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
    for r in rects {
        if let Some(a) = r.as_array()
            && a.len() == 4
        {
            let x = a[0].as_f64().unwrap_or(0.0).max(0.0) as u32;
            let y = a[1].as_f64().unwrap_or(0.0).max(0.0) as u32;
            let rw = a[2].as_f64().unwrap_or(0.0).max(0.0) as u32;
            let rh = a[3].as_f64().unwrap_or(0.0).max(0.0) as u32;
            for yy in y..(y + rh).min(h) {
                for xx in x..(x + rw).min(w) {
                    mask.put_pixel(xx, yy, image::Rgba([0, 0, 0, 0]));
                }
            }
        }
    }
    mask
}

pub(super) async fn generate_image(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let prompt = arg_str(args, "prompt")?;
    let name = asset_stem(&arg_str(args, "name")?);
    let quality = args["quality"]
        .as_str()
        .map(str::to_string)
        .or_else(|| plan_quality(ctx, |q| q.figure))
        .unwrap_or_else(|| "high".into());
    let transparent = args["transparent"].as_bool().unwrap_or(false);
    let refs: Vec<String> = args["references"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let (references, mask_png, size) = {
        let h = lock(&ctx.handle);
        let mut references = Vec::new();
        for r in &refs {
            references.push(resolve_reference(&h, ctx.facts.as_ref(), r)?);
        }
        let first_dims = references
            .first()
            .and_then(|b| cimg::from_png_bytes(b).ok())
            .map(|i| (i.width(), i.height()));
        let mask_png = match (args["mask_rects_px"].as_array(), first_dims) {
            (Some(rects), Some((w, hh))) => Some(cimg::to_png_bytes(&repaint_mask(w, hh, rects))?),
            _ => None,
        };
        let size = match (args["width_px"].as_u64(), args["height_px"].as_u64()) {
            (Some(w), Some(hh)) => Some(clamp_gen_size(w as u32, hh as u32)),
            _ => first_dims.map(|(w, hh)| {
                clamp_gen_size(
                    w.max(1024),
                    (hh as f64 * (w.max(1024) as f64 / w as f64)).round() as u32,
                )
            }),
        };
        (references, mask_png, size)
    };
    let out = request_image(
        ctx,
        ImageRequest {
            model: ctx.image_model.clone(),
            prompt: if transparent {
                clean_prompt(&prompt)
            } else {
                clean_prompt(&format!(
                    "{}. {FILL_SUFFIX}",
                    prompt.trim_end().trim_end_matches('.')
                ))
            },
            size,
            quality,
            transparent,
            references,
            mask_png,
        },
    )
    .await?;
    let mut im = cimg::from_png_bytes(&out.png)?;
    if !transparent {
        im = trim_bars(im);
    }
    let (asset, preview) = {
        let h = lock(&ctx.handle);
        let asset = h.new_asset_name(&name, "png");
        cimg::save_png(&im, &h.asset_path(&asset))?;
        (asset, cimg::to_png_bytes(&cimg::resize_max(&im, 768))?)
    };
    Ok(ToolResult::with_image(
        json!({"asset": asset, "width_px": im.width(), "height_px": im.height(), "transparent": transparent, "prompt": prompt}),
        preview,
    ))
}

pub(super) async fn extend_background(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let asset = arg_str(args, "asset")?;
    let hint = args["prompt"].as_str().unwrap_or("").trim().to_string();
    let prompt = format!(
        "Continue the existing background seamlessly into the transparent area, as if the same picture simply kept going. Keep the same palette, color strength, brightness, gradients, lighting, horizon and texture; never fade toward white or add a bright haze. Any leaf, cloud, plant or shape that is cut off at the edge of the existing picture must continue naturally across the boundary and finish; where the edge is plain, stay plain and soft. No text, no logos, no borders, no new objects or motifs that are not already present. The second image shows the whole background so far for style reference.{}",
        if hint.is_empty() {
            String::new()
        } else {
            format!(" Scenery hint: {hint}.")
        }
    );
    // Default height: enough for the background dpi requirement (half the target) so one pass is enough.
    let needed_h = {
        let h = lock(&ctx.handle);
        (h.document.artboard.height_mm / 25.4 * h.project.target_dpi / 2.0).ceil() as u64
    };
    let target_h = extend_height(Some(
        args["height_px"].as_u64().unwrap_or(0).max(needed_h).max(1280),
    ));
    let quality = plan_quality(ctx, |q| q.background).unwrap_or_else(|| "medium".into());
    let (mut cur, ratio) = {
        let h = lock(&ctx.handle);
        let im = cimg::load(&h.asset_path(&asset))?;
        let ratio = h.document.artboard.width_mm / h.document.artboard.height_mm.max(1.0);
        let scaled = cimg::resize_to(
            &im,
            ((im.width() as f64) * target_h as f64 / im.height() as f64)
                .round()
                .max(1.0) as u32,
            target_h,
        );
        (scaled, ratio)
    };
    let target_w = (target_h as f64 * ratio).round() as u32;
    if cur.width() >= target_w {
        let x = (cur.width() - target_w) / 2;
        cur = cimg::crop(&cur, [x, 0, target_w, target_h]);
    } else {
        let prompt = clean_prompt(&prompt);
        cur = outpaint(cur, target_w, &ctx.run.stop, |canvas, mask, plate| {
            let req = ImageRequest {
                model: ctx.image_model.clone(),
                prompt: prompt.clone(),
                size: Some(canvas.dimensions()),
                quality: quality.clone(),
                transparent: false,
                references: vec![],
                mask_png: None,
            };
            async move {
                let req = ImageRequest {
                    references: vec![cimg::to_png_bytes(&canvas)?, cimg::to_png_bytes(&plate)?],
                    mask_png: Some(cimg::to_png_bytes(&mask)?),
                    ..req
                };
                let out = request_image(ctx, req).await?;
                Ok(cimg::from_png_bytes(&out.png)?)
            }
        })
        .await?;
    }
    // A plate is opaque by definition; any alpha left over would let the white under it show as haze.
    for p in cur.pixels_mut() {
        p.0[3] = 255;
    }
    let (out_asset, preview) = {
        let h = lock(&ctx.handle);
        let name = h.new_asset_name("background", "png");
        cimg::save_png(&cur, &h.asset_path(&name))?;
        (name, cimg::to_png_bytes(&cimg::resize_max(&cur, 1024))?)
    };
    Ok(ToolResult::with_image(
        json!({"asset": out_asset, "width_px": cur.width(), "height_px": cur.height(), "note": "Place it with add_layer(kind=image, strategy=background, parent '배경', x/y/w/h null) so it covers the whole artboard."}),
        preview,
    ))
}

/// The image API wants multiples of 16; rounded up so the plate does not land just under the background dpi requirement.
fn extend_height(requested: Option<u64>) -> u32 {
    (requested.unwrap_or(1280).clamp(512, 2160) as u32).div_ceil(16) * 16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_sizes_stay_inside_the_pixel_budget() {
        for (w, h) in [
            (3219, 3840),
            (3840, 3840),
            (3840, 1280),
            (2600, 3100),
            (1000, 1000),
            (16, 4000),
        ] {
            let (gw, gh) = clamp_gen_size(w, h);
            assert!(gw * gh <= MAX_GEN_PX, "{w}x{h} -> {gw}x{gh}");
            assert!(
                gw % 16 == 0 && gh % 16 == 0 && gw.max(gh) <= 3840,
                "{w}x{h} -> {gw}x{gh}"
            );
        }
    }

    #[test]
    fn extend_height_is_a_multiple_of_16_within_limits() {
        assert_eq!(extend_height(Some(1400)), 1408);
        assert_eq!(extend_height(Some(1772)), 1776);
        assert_eq!(extend_height(Some(2160)), 2160);
        assert_eq!(extend_height(Some(5000)), 2160);
        assert_eq!(extend_height(Some(100)), 512);
        assert_eq!(extend_height(None), 1280);
        assert!((512..=2160u64).all(|h| extend_height(Some(h)).is_multiple_of(16)));
    }
}
