use std::sync::atomic::Ordering;

use anyhow::bail;
use hoist_core::compare::mask_overlap;
use hoist_core::model::*;
use hoist_core::{image as cimg, matting, trace};
use serde_json::{Value, json};

use super::edit::{edit_result, ensure_group};
use super::{
    ToolCtx, ToolResult, apply_as_agent, arg_rect_px, arg_str, asset_stem, ensure_running, r3,
    require_original, tidy_name, version_of, with_fields,
};
use crate::agent::img;
use crate::agent::prompt::OriginalFacts;
use crate::state::lock;

const TRACE_MAX_UPSAMPLED_PX: f64 = 16_000_000.0;

fn trace_factor_limit(rect: [u32; 4]) -> u32 {
    let area = f64::from(rect[2].max(1)) * f64::from(rect[3].max(1));
    ((TRACE_MAX_UPSAMPLED_PX / area).sqrt().floor() as u32).clamp(1, 8)
}

pub(super) fn trace_region(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let f = require_original(ctx)?;
    let version = version_of(ctx, args)?;
    let rect = arg_rect_px(args, "rect_px")?;
    let color = arg_str(args, "color")?;
    let tol = args["tolerance"].as_u64().unwrap_or(40) as u8;
    let up = (args["upsample"]
        .as_u64()
        .unwrap_or(u64::from(trace::SMOOTH_FACTOR))
        .clamp(1, 8) as u32)
        .min(trace_factor_limit(rect));
    let name = tidy_name(&arg_str(args, "name")?);
    let strategy: Strategy = serde_json::from_value(args["strategy"].clone()).unwrap_or(Strategy::TextTraced);
    let reason = args["strategy_reason"].as_str().unwrap_or("").to_string();
    let group = args["group"].as_str().map(|s| s.to_string());
    let crop = {
        let h = lock(&ctx.handle);
        cimg::crop(&img::original_image(&h, version)?, rect)
    };
    let coverage = trace::color_coverage(&crop, &color, tol);
    let mask_up = trace::smooth_mask(&coverage, up, trace::SMOOTH_SIGMA_SOURCE_PX);
    let opts = trace::TraceOptions::upsampled(f.mm_per_px, up);
    let d = trace::trace_mask(&mask_up, &opts)?;
    let back = trace::rasterize_path(&d, mask_up.width(), mask_up.height(), 1.0 / opts.mm_per_px)?;
    let iou = mask_overlap(&mask_up, &back);
    let layer_id = {
        let mut h = lock(&ctx.handle);
        ensure_running(ctx)?;
        let parent = match group {
            Some(g) => Some(ensure_group(&mut h, &g, &ctx.turn)?),
            None => None,
        };
        let id = h.document.next_id();
        let [x_mm, y_mm, w_mm, h_mm] = original_rect_mm(f, rect.map(f64::from));
        let layer = Layer {
            id: id.clone(),
            name: name.clone(),
            parent,
            kind: LayerKind::Path(PathProps {
                d: d.clone(),
                fill_rule: FillRule::Nonzero,
            }),
            x_mm,
            y_mm,
            w_mm,
            h_mm,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![Paint::Solid {
                color: color.clone(),
                opacity: 1.0,
            }],
            strokes: vec![],
            effects: vec![],
            strategy,
            strategy_reason: if reason.is_empty() {
                format!("원본 모양 트레이싱 (iou {:.2})", iou)
            } else {
                reason
            },
            pinned: vec![],
            source: Some(LayerSource {
                original_version: Some(version),
                crop_px: Some(rect),
                asset: None,
                prompt: None,
            }),
        };
        apply_as_agent(
            &mut h,
            &ctx.turn,
            vec![Op::AddLayer {
                layer: Box::new(layer),
                index: None,
            }],
        )?;
        id
    };
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    let res = edit_result(ctx, &layer_id, &format!("레이어 {layer_id} 트레이싱 추가"))?;
    let mut extra = json!({"layer_id": layer_id, "iou": r3(iou), "path_len": d.len()});
    if iou < 0.9 {
        extra["hint"] =
            json!("iou가 0.9 미만입니다. tolerance를 조정해 다시 따거나 generated_raster로 승격하세요.");
    }
    Ok(with_fields(res, extra))
}

pub(super) fn crop_original(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let version = version_of(ctx, args)?;
    let rect = arg_rect_px(args, "rect_px")?;
    let name = asset_stem(&arg_str(args, "name")?);
    let h = lock(&ctx.handle);
    let orig = img::original_image(&h, version)?;
    let mut crop = cimg::crop(&orig, rect);
    if let Some(c) = args["transparent_color"].as_str() {
        let tol = args["tolerance"].as_u64().unwrap_or(30) as u8;
        let mask = trace::color_mask(&crop, c, tol);
        for (x, y, p) in crop.enumerate_pixels_mut() {
            if mask.get_pixel(x, y)[0] > 127 {
                p[3] = 0;
            }
        }
    }
    let asset = h.new_asset_name(&name, "png");
    cimg::save_png(&crop, &h.asset_path(&asset))?;
    let f = require_original(ctx)?;
    Ok(ToolResult::with_image(
        json!({"asset": asset, "width_px": crop.width(), "height_px": crop.height(), "place_mm": original_rect_mm(f, rect.map(f64::from))}),
        cimg::to_png_bytes(&cimg::resize_max(&crop, 512))?,
    ))
}

pub(super) fn cut_out(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    if !matting::available(&ctx.models_dir) {
        bail!(
            "누끼 모델이 설치돼 있지 않아 cut_out을 쓸 수 없습니다. 사용자에게는 'Hoist를 다시 설치하면 누끼 따기가 됩니다'라고 알리고, 이번에는 crop_original(transparent_color)이나 generate_image(transparent=true)로 대신하세요."
        );
    }
    let version = version_of(ctx, args)?;
    let rect = arg_rect_px(args, "rect_px")?;
    let name = asset_stem(&arg_str(args, "name")?);
    let (crop, target_dpi) = {
        let h = lock(&ctx.handle);
        (
            cimg::crop(&img::original_image(&h, version)?, rect),
            h.project.target_dpi,
        )
    };
    if crop.width() == 0 || crop.height() == 0 {
        bail!("rect_px가 원본 밖입니다");
    }
    let cut = matting::remove_background(&ctx.models_dir, &crop)?;
    let asset = {
        let h = lock(&ctx.handle);
        let asset = h.new_asset_name(&name, "png");
        cimg::save_png(&cut, &h.asset_path(&asset))?;
        asset
    };
    let f = require_original(ctx)?;
    let place = original_rect_mm(
        f,
        [
            f64::from(rect[0]),
            f64::from(rect[1]),
            f64::from(cut.width()),
            f64::from(cut.height()),
        ],
    );
    let dpi = f64::from(cut.width()) * 25.4 / place[2];
    let note = if dpi < target_dpi {
        format!(
            "Print resolution {dpi:.0} dpi is below the target {target_dpi:.0} dpi. Do not enlarge this piece: redraw it with generate_image(transparent=true, references=[this asset], same aspect, long edge 3840 px), then compare the result with the piece and redraw if shape or colors differ."
        )
    } else {
        "Place it with add_layer(kind=image, strategy=original_crop, strategy_reason='원본에서 따서 배경 제거') at place_mm.".into()
    };
    Ok(ToolResult::with_image(
        json!({"asset": asset, "width_px": cut.width(), "height_px": cut.height(), "place_mm": place, "dpi": dpi.round(), "note": note}),
        cimg::to_png_bytes(&cimg::resize_max(&cut, 512))?,
    ))
}

fn original_rect_mm(f: &OriginalFacts, px: [f64; 4]) -> [f64; 4] {
    [
        f.offset_x_mm + px[0] * f.mm_per_px,
        f.offset_y_mm + px[1] * f.mm_per_px,
        px[2] * f.mm_per_px,
        px[3] * f.mm_per_px,
    ]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::agent::tools::execute_sync;
    use crate::agent::tools::tests::test_ctx;

    #[test]
    fn cut_out_without_the_model_says_how_to_continue() {
        let ctx = test_ctx(std::env::temp_dir().join("hoist-no-models"));
        let err = execute_sync(&ctx, "cut_out", json!({"rect_px": [0, 0, 100, 100], "name": "x"}))
            .err()
            .expect("no model")
            .to_string();
        assert!(
            err.contains("다시 설치") && err.contains("crop_original"),
            "{err}"
        );
    }

    #[test]
    fn a_stopped_trace_adds_no_group() {
        let ctx = test_ctx(std::env::temp_dir().join("hoist-no-models"));
        ctx.run.request_stop();
        let err = execute_sync(
            &ctx,
            "trace_region",
            json!({"rect_px": [250, 50, 500, 500], "color": "#E62828", "name": "원", "group": "그래픽"}),
        )
        .err()
        .expect("stopped")
        .to_string();
        assert!(err.contains("중단"), "{err}");
        assert!(lock(&ctx.handle).document.layers.is_empty());
    }

    #[test]
    #[ignore = "needs scripts/fetch-models.ps1"]
    fn cut_out_removes_the_background_and_places_the_piece() {
        let ctx = test_ctx(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models"));
        let args = json!({"rect_px": [250, 50, 500, 500], "name": "disc", "version": null});
        let res = execute_sync(&ctx, "cut_out", args.clone()).expect("cut_out");
        let v: Value = serde_json::from_str(&res.text).expect("json");
        assert_eq!(res.images.len(), 1);
        assert_eq!(
            (v["width_px"].as_u64(), v["height_px"].as_u64()),
            (Some(500), Some(500))
        );
        let place: Vec<f64> = serde_json::from_value(v["place_mm"].clone()).expect("place");
        assert_eq!(place, [150.0, 30.0, 300.0, 300.0]);
        assert_eq!(v["dpi"].as_f64(), Some(42.0));
        assert!(
            v["note"].as_str().is_some_and(|n| n.contains("generate_image")),
            "{v}"
        );
        let asset = v["asset"].as_str().expect("asset");
        let cut = cimg::load(&lock(&ctx.handle).asset_path(asset)).expect("saved");
        assert!(cut.get_pixel(250, 250)[3] > 230, "disc stays");
        assert!(cut.get_pixel(5, 5)[3] < 25, "background goes");
        let again = execute_sync(&ctx, "cut_out", args).expect("again");
        assert!(again.cached && again.images.is_empty());
        lock(&ctx.handle).project.target_dpi = 60.0;
        let short = execute_sync(
            &ctx,
            "cut_out",
            json!({"rect_px": [250, 50, 499, 500], "name": "disc"}),
        )
        .expect("below target");
        assert!(
            short.text.contains("generate_image"),
            "42dpi < 60dpi regenerates: {}",
            short.text
        );
        lock(&ctx.handle).project.target_dpi = 40.0;
        let sharp = execute_sync(
            &ctx,
            "cut_out",
            json!({"rect_px": [250, 50, 500, 499], "name": "disc"}),
        )
        .expect("enough dpi");
        assert!(sharp.text.contains("original_crop"), "{}", sharp.text);
    }
}
