use std::sync::atomic::Ordering;

use anyhow::{anyhow, bail};
use hoist_core::boolean::{self, BooleanOp, fit_path_d, scale_path_d};
use hoist_core::image as cimg;
use hoist_core::model::*;
use hoist_core::render::layer_bounds_mm;
use hoist_core::store::ProjectHandle;
use serde_json::{Value, json};

use super::text_box::{hug_text_box, ink_mm, keep_one_line};
use super::turn::wording_only_patch;
use super::{
    ToolCtx, ToolResult, apply_as_agent, arg_str, ensure_running, layer_by_id, r1, r3, tidy_name, with_fields,
};
use crate::agent::{img, layer_args};
use crate::state::lock;

pub(super) fn add_layer(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let artboard = {
        let h = lock(&ctx.handle);
        (h.document.artboard.width_mm, h.document.artboard.height_mm)
    };
    let mut layer = layer_args::parse_layer(&args["layer"], artboard)?;
    layer.name = tidy_name(&layer.name);
    if let Some([x, w, h]) = hug_text_box(&ctx.fonts, &layer) {
        (layer.x_mm, layer.w_mm, layer.h_mm) = (x, w, h);
    }
    let index = args["index"].as_u64().map(|v| v as usize);
    let touched = {
        let mut h = lock(&ctx.handle);
        ensure_running(ctx)?;
        insert_layer(&mut h, layer, index, &ctx.turn)?
    };
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    edit_result(ctx, &touched, &format!("레이어 {touched} 추가"))
}

pub(super) fn update_layer(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let id = arg_str(args, "id")?;
    let mut patch = layer_args::patch_from_args(&args["patch"]);
    if let Some(name) = patch.get_mut("name")
        && let Some(s) = name.as_str()
    {
        *name = json!(tidy_name(s));
    }
    if patch.as_object().is_none_or(|m| m.is_empty()) {
        bail!("patch에 바꿀 필드가 없습니다. 바꿀 필드만 값을 넣고 나머지는 null로 보내세요.");
    }
    if patch.get("strategy").is_some() {
        ctx.state.promoted.fetch_add(1, Ordering::SeqCst);
    }
    {
        let mut h = lock(&ctx.handle);
        ensure_running(ctx)?;
        let artboard_w = h.document.artboard.width_mm;
        if let Some(layer) = h.document.layer(&id) {
            if wording_only_patch(layer, &patch) {
                ctx.state.wording_edits.fetch_add(1, Ordering::SeqCst);
            }
            keep_path_in_its_box(layer, &mut patch);
            keep_one_line(&ctx.fonts, layer, artboard_w, &mut patch);
        }
        apply_as_agent(
            &mut h,
            &ctx.turn,
            vec![Op::UpdateLayer {
                id: id.clone(),
                patch,
            }],
        )?;
        if let Some([x, w, h_mm]) = h.document.layer(&id).and_then(|l| hug_text_box(&ctx.fonts, l)) {
            apply_as_agent(
                &mut h,
                &ctx.turn,
                vec![Op::UpdateLayer {
                    id: id.clone(),
                    patch: json!({"x_mm": x, "w_mm": w, "h_mm": h_mm}),
                }],
            )?;
        }
    }
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    edit_result(ctx, &id, &format!("레이어 {id} 수정"))
}

pub(super) fn path_boolean(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let doc = lock(&ctx.handle).document.clone();
    let ids: Vec<LayerId> = serde_json::from_value(args["layer_ids"].clone())
        .map_err(|_| anyhow!("layer_ids는 레이어 id 배열이어야 합니다"))?;
    let op: BooleanOp = serde_json::from_value(args["op"].clone())
        .map_err(|_| anyhow!("op는 union·subtract·intersect·exclude 중 하나입니다"))?;
    let first = ids
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("layer_ids가 비었습니다"))?;
    let ops = boolean::path_boolean(&doc, &ctx.fonts, &ids, op)?;
    apply_path_ops(ctx, ops, &first, &format!("{} 패스 합치기", ids.join("+")))
}

pub(super) fn outline_text(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let doc = lock(&ctx.handle).document.clone();
    let id = arg_str(args, "layer_id")?;
    let ops = boolean::outline_text(&doc, &ctx.fonts, &id)?;
    apply_path_ops(ctx, ops, &id, &format!("{id} 글자 윤곽선"))
}

fn apply_path_ops(ctx: &ToolCtx, ops: Vec<Op>, id: &str, note: &str) -> anyhow::Result<ToolResult> {
    {
        let mut h = lock(&ctx.handle);
        ensure_running(ctx)?;
        apply_as_agent(&mut h, &ctx.turn, ops)?;
    }
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    edit_result(ctx, id, note)
}

pub(super) fn delete_layer(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let id = arg_str(args, "id")?;
    let bounds = {
        let mut h = lock(&ctx.handle);
        let b = h.document.layer(&id).map(|l| layer_bounds_mm(&h.document, l));
        ensure_running(ctx)?;
        apply_as_agent(&mut h, &ctx.turn, vec![Op::DeleteLayer { id: id.clone() }])?;
        b
    };
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    match bounds {
        Some(b) => region_result(ctx, b, &format!("레이어 {id} 삭제")),
        None => Ok(ToolResult::text(format!("레이어 {id} 삭제"))),
    }
}

pub(super) fn move_layer(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let id = arg_str(args, "id")?;
    let parent = args["parent"].as_str().map(|s| s.to_string());
    let index = args["index"].as_u64().unwrap_or(0) as usize;
    {
        let mut h = lock(&ctx.handle);
        ensure_running(ctx)?;
        let parent = match parent {
            Some(p) if h.document.layer(&p).is_none() => Some(ensure_group(&mut h, &p, &ctx.turn)?),
            other => other,
        };
        apply_as_agent(
            &mut h,
            &ctx.turn,
            vec![Op::MoveLayer {
                id: id.clone(),
                parent,
                index,
            }],
        )?;
    }
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    Ok(ToolResult::text(format!("레이어 {id} 이동")))
}

fn insert_layer(
    h: &mut ProjectHandle,
    mut layer: Layer,
    index: Option<usize>,
    turn: &str,
) -> anyhow::Result<LayerId> {
    if let Some(p) = layer.parent.clone()
        && h.document.layer(&p).is_none()
    {
        layer.parent = Some(ensure_group(h, &p, turn)?);
    }
    if layer.id.is_empty() || h.document.layer(&layer.id).is_some() {
        layer.id = h.document.next_id();
    }
    let id = layer.id.clone();
    apply_as_agent(
        h,
        turn,
        vec![Op::AddLayer {
            layer: Box::new(layer),
            index,
        }],
    )?;
    Ok(id)
}

pub(super) fn ensure_group(h: &mut ProjectHandle, name: &str, turn: &str) -> anyhow::Result<LayerId> {
    let name = tidy_name(name);
    let name = name.as_str();
    if let Some(g) = h
        .document
        .layers
        .iter()
        .find(|l| l.parent.is_none() && matches!(l.kind, LayerKind::Group) && tidy_name(&l.name) == name)
    {
        return Ok(g.id.clone());
    }
    let id = h.document.next_id();
    let layer = Layer {
        id: id.clone(),
        name: name.to_string(),
        parent: None,
        kind: LayerKind::Group,
        x_mm: 0.0,
        y_mm: 0.0,
        w_mm: h.document.artboard.width_mm,
        h_mm: h.document.artboard.height_mm,
        rotation_deg: 0.0,
        opacity: 1.0,
        visible: true,
        locked: false,
        fills: vec![],
        strokes: vec![],
        effects: vec![],
        strategy: Strategy::NativeVector,
        strategy_reason: "그룹".into(),
        pinned: vec![],
        source: None,
    };
    let index = match name {
        "배경" => Some(0),
        _ => None,
    };
    apply_as_agent(
        h,
        turn,
        vec![Op::AddLayer {
            layer: Box::new(layer),
            index,
        }],
    )?;
    Ok(id)
}

/// A path draws its `d` as written, so a resized box has to carry a resized outline.
fn keep_path_in_its_box(layer: &Layer, patch: &mut Value) {
    let LayerKind::Path(p) = &layer.kind else {
        return;
    };
    if patch["kind"]["type"].as_str().is_some_and(|t| t != "path") {
        return;
    }
    let w = patch["w_mm"].as_f64().unwrap_or(layer.w_mm);
    let h = patch["h_mm"].as_f64().unwrap_or(layer.h_mm);
    let d = match patch["kind"]["d"].as_str() {
        Some(d) => fit_path_d(d, w, h),
        None if w != layer.w_mm || h != layer.h_mm => scale_path_d(&p.d, w / layer.w_mm, h / layer.h_mm),
        None => return,
    };
    if let Ok(d) = d {
        patch["kind"]["d"] = json!(d);
    }
}

pub(super) fn edit_result(ctx: &ToolCtx, layer_id: &str, note: &str) -> anyhow::Result<ToolResult> {
    let (bounds, layer) = {
        let h = lock(&ctx.handle);
        let l = layer_by_id(&h.document, layer_id)?;
        (layer_bounds_mm(&h.document, l), l.clone())
    };
    let res = region_result(ctx, bounds, note)?;
    if !matches!(layer.kind, LayerKind::Text(_)) {
        return Ok(res);
    }
    let (ink, lines) = ink_mm(&ctx.fonts, &layer);
    Ok(with_fields(res, json!({"ink_mm": ink.map(r1), "lines": lines})))
}

pub(super) fn region_result(ctx: &ToolCtx, bounds: [f64; 4], note: &str) -> anyhow::Result<ToolResult> {
    let pad_w = bounds[2] * 0.15 + 10.0;
    let pad_h = bounds[3] * 0.15 + 10.0;
    let (doc, assets, original_path, rect) = {
        let h = lock(&ctx.handle);
        let rect = img::clamp_rect_mm(
            &h,
            [
                bounds[0] - pad_w,
                bounds[1] - pad_h,
                bounds[2] + pad_w * 2.0,
                bounds[3] + pad_h * 2.0,
            ],
        );
        let original_path = ctx.facts.as_ref().map(|f| h.original_path(f.version));
        (h.document.clone(), h.assets_dir(), original_path, rect)
    };
    match (ctx.facts.as_ref(), original_path) {
        (Some(f), Some(path)) => {
            let original = cimg::load(&path)?;
            let (res, png) = img::compare_region(&doc, &assets, &original, &ctx.fonts, f, rect, 900)?;
            Ok(ToolResult::with_image(
                json!({"note": note, "region_mm": rect, "similarity": r3(res.similarity), "overlap": r3(res.overlap), "edge_similarity": r3(res.edge_similarity), "image": "왼쪽 원본 · 오른쪽 작업본"}),
                png,
            ))
        }
        _ => {
            let im = img::work_region(&doc, &assets, &ctx.fonts, rect, 900)?;
            Ok(ToolResult::with_image(
                json!({"note": note, "region_mm": rect}),
                cimg::to_png_bytes(&im)?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::tests::{rect_layer, test_ctx};
    use crate::agent::tools::{execute_sync, label_for};

    #[test]
    fn resizing_a_path_layer_scales_its_outline() {
        let raw = json!({"name": "붓 띠", "kind": {"type": "path", "d": "M 0 0 L 100 0 L 100 20 L 0 20 Z"}, "x_mm": 0, "y_mm": 0, "w_mm": 100, "h_mm": 20});
        let layer = layer_args::parse_layer(&raw, (6000.0, 900.0)).expect("path layer");
        let mut moved = json!({"x_mm": 500});
        keep_path_in_its_box(&layer, &mut moved);
        assert_eq!(moved, json!({"x_mm": 500}), "a move leaves the outline alone");
        let mut resized = json!({"w_mm": 200, "h_mm": 10});
        keep_path_in_its_box(&layer, &mut resized);
        let d = resized["kind"]["d"].as_str().expect("scaled outline");
        assert!(d.contains("200 0") && d.contains("200 10"), "{d}");
        let mut redrawn = json!({"kind": {"type": "path", "d": "M 0 0 L 5 0 L 5 5 Z"}});
        keep_path_in_its_box(&layer, &mut redrawn);
        let d = redrawn["kind"]["d"].as_str().expect("fitted outline");
        assert!(d.contains("100 0") && d.contains("100 20"), "{d}");
    }

    #[test]
    fn a_new_parent_group_and_its_layer_get_different_ids() {
        let root = std::env::temp_dir().join(format!("hoist-ids-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root)
            .create("ids", 600.0, 300.0)
            .expect("project");
        let mut layer = rect_layer("", 0.0);
        layer.parent = Some("배경".into());
        let id = insert_layer(&mut h, layer, Some(0), "t").expect("empty id");
        let group = |doc: &Document, name: &str| {
            doc.layers
                .iter()
                .find(|l| l.name == name)
                .map(|l| l.id.clone())
                .expect("group")
        };
        let background = group(&h.document, "배경");
        assert_ne!(id, background);
        assert_eq!(
            h.document.layer(&id).and_then(|l| l.parent.clone()),
            Some(background)
        );

        let mut layer = rect_layer("", 0.0);
        layer.id = h.document.next_id();
        layer.parent = Some("글자".into());
        let id = insert_layer(&mut h, layer, None, "t").expect("id the new group takes");
        assert_ne!(id, group(&h.document, "글자"));
        assert_eq!(h.document.layers.len(), 4);
    }

    #[test]
    fn path_tools_merge_shapes_and_outline_text() {
        let ctx = test_ctx(std::env::temp_dir().join("hoist-no-models"));
        let shape = rect_layer;
        let mut words = shape("t", 50.0);
        words.kind = LayerKind::Text(TextProps {
            text: "가나".into(),
            font_family: "Pretendard".into(),
            font_size_mm: 30.0,
            ..Default::default()
        });
        let ops = ["a", "b"]
            .iter()
            .zip([50.0, 120.0])
            .map(|(id, x)| shape(id, x))
            .chain([words])
            .map(|l| Op::AddLayer {
                layer: Box::new(l),
                index: None,
            })
            .collect();
        lock(&ctx.handle).apply_ops(ops, Actor::User, None).expect("seed");
        let rev = || lock(&ctx.handle).document.rev;

        let seeded = rev();
        execute_sync(
            &ctx,
            "path_boolean",
            json!({"layer_ids": ["a", "b"], "op": "union"}),
        )
        .expect("union");
        let merged = rev();
        assert!(merged > seeded, "the canvas hears about a merge");
        execute_sync(&ctx, "outline_text", json!({"layer_id": "t"})).expect("outline");
        assert!(rev() > merged, "the canvas hears about outlined text");
        let h = lock(&ctx.handle);
        assert!(matches!(
            h.document.layer("a").map(|l| &l.kind),
            Some(LayerKind::Path(_))
        ));
        assert!(h.document.layer("b").is_none(), "merged away");
        assert!(matches!(
            h.document.layer("t").map(|l| &l.kind),
            Some(LayerKind::Path(_))
        ));
        assert_eq!(ctx.state.edits.load(Ordering::SeqCst), 2);
        assert_eq!(
            label_for("path_boolean", &json!({"op": "subtract"})),
            "패스 합치기 · 빼기"
        );
    }
}
