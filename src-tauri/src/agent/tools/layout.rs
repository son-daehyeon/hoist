use std::sync::atomic::Ordering;

use anyhow::{anyhow, bail};
use hoist_core::model::{LayerKind, Op};
use hoist_core::render::layer_bounds_mm;
use serde_json::{Value, json};

use super::edit::{edit_result, region_result};
use super::text_box::{ink_mm, union_mm};
use super::{
    ToolCtx, ToolResult, apply_as_agent, arg_str, ensure_running, find_layer, layer_by_id, r1, with_fields,
};
use crate::state::lock;

fn layer_refs(v: &Value, key: &str) -> anyhow::Result<Vec<String>> {
    let refs: Vec<String> = serde_json::from_value(v.clone())
        .map_err(|_| anyhow!("{key} must be an array of layer ids or names"))?;
    if refs.is_empty() {
        bail!("{key} must name at least one layer");
    }
    Ok(refs)
}

pub(super) fn arrange_row(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let items = args["items"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| anyhow!("items must hold at least one item"))?
        .iter()
        .map(|item| layer_refs(item, "each item"))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let number = |key: &str| {
        args[key]
            .as_f64()
            .ok_or_else(|| anyhow!("{key} must be a number"))
    };
    let (x0, x1) = (number("x0_mm")?, number("x1_mm")?);
    let justify = args["justify"].as_str().unwrap_or("");
    if !["center", "start", "end", "space_between"].contains(&justify) {
        bail!("justify must be center, start, end or space_between");
    }
    let align_y = args["align_y"].as_str().unwrap_or("");
    if !["", "center", "top"].contains(&align_y) {
        bail!("align_y must be center, top or null");
    }
    let n = items.len();
    let mut moved = Vec::new();
    let result = {
        let mut h = lock(&ctx.handle);
        ensure_running(ctx)?;
        let doc = &h.document;
        let mut rows = Vec::new();
        for refs in &items {
            let layers = refs
                .iter()
                .map(|r| find_layer(doc, r))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let ink = union_mm(layers.iter().map(|l| ink_mm(&ctx.fonts, l).0)).unwrap_or_default();
            rows.push((layers, ink));
        }
        let range = x1 - x0;
        let content: f64 = rows.iter().map(|(_, b)| b[2]).sum();
        let spread = justify == "space_between" && n > 1;
        let gap = if spread {
            (range - content) / (n - 1) as f64
        } else {
            args["gap_mm"].as_f64().unwrap_or(0.0)
        };
        let total = content + gap * (n - 1) as f64;
        let start = match justify {
            "start" => x0,
            "end" => x1 - total,
            _ if spread => x0,
            _ => x0 + (range - total) / 2.0,
        };
        let mut ops = Vec::new();
        let mut out = Vec::new();
        let mut centers = Vec::new();
        let mut cursor = start;
        let tallest = rows
            .iter()
            .map(|(_, b)| *b)
            .max_by(|a, b| a[3].total_cmp(&b[3]))
            .unwrap_or_default();
        for (i, (layers, b)) in rows.iter().enumerate() {
            let dx = cursor - b[0];
            let dy = match align_y {
                "center" => (tallest[1] + tallest[3] / 2.0) - (b[1] + b[3] / 2.0),
                "top" => tallest[1] - b[1],
                _ => 0.0,
            };
            for l in layers {
                ops.push(Op::UpdateLayer {
                    id: l.id.clone(),
                    patch: if dy == 0.0 {
                        json!({"x_mm": l.x_mm + dx})
                    } else {
                        json!({"x_mm": l.x_mm + dx, "y_mm": l.y_mm + dy})
                    },
                });
                moved.push(l.id.clone());
            }
            let names: Vec<&str> = layers.iter().map(|l| l.name.as_str()).collect();
            let ink = [b[0] + dx, b[1] + dy, b[2], b[3]].map(r1);
            out.push(json!({"layers": names, "ink_mm": ink}));
            cursor += b[2];
            if i + 1 < n {
                centers.push(r1(cursor + gap / 2.0));
            }
            cursor += gap;
        }
        let mut result = json!({"items": out, "content_mm": r1(content), "gap_mm": r1(gap), "row_mm": [r1(start), r1(start + total)], "gap_centers_mm": centers, "free_mm": r1(range - total)});
        if total > range + 0.5 || gap < 0.0 {
            result["overflow"] = json!(format!(
                "content is {} mm wider than the range: reduce the font size or re-break lines, then call again",
                r1((content - range).max(total - range))
            ));
        }
        apply_as_agent(&mut h, &ctx.turn, ops)?;
        result
    };
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    let bounds = {
        let h = lock(&ctx.handle);
        union_mm(
            moved
                .iter()
                .filter_map(|id| h.document.layer(id))
                .map(|l| layer_bounds_mm(&h.document, l)),
        )
        .unwrap_or_default()
    };
    let res = region_result(ctx, bounds, &format!("{n}개 항목 줄 맞춤"))?;
    Ok(with_fields(res, result))
}

pub(super) fn fit_box(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let target = arg_str(args, "id")?;
    let around = layer_refs(&args["around"], "around")?;
    let (pad_x, pad_y) = (
        args["pad_x_mm"].as_f64().unwrap_or(0.0),
        args["pad_y_mm"].as_f64().unwrap_or(0.0),
    );
    let axis = args["axis"].as_str().unwrap_or("both");
    if !["both", "x", "y"].contains(&axis) {
        bail!("axis must be both, x or y");
    }
    let (id, content, placed) = {
        let mut h = lock(&ctx.handle);
        ensure_running(ctx)?;
        let doc = &h.document;
        let shape = find_layer(doc, &target)?;
        if !matches!(shape.kind, LayerKind::Shape(_)) {
            bail!(
                "'{}' is not a shape layer: fit_box only resizes shapes",
                shape.name
            );
        }
        let id = shape.id.clone();
        let layers = around
            .iter()
            .map(|r| find_layer(doc, r))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let b = union_mm(layers.iter().map(|l| ink_mm(&ctx.fonts, l).0)).unwrap_or_default();
        let mut patch = serde_json::Map::new();
        if axis != "y" {
            patch.insert("x_mm".into(), json!(b[0] - pad_x));
            patch.insert("w_mm".into(), json!(b[2] + 2.0 * pad_x));
        }
        if axis != "x" {
            patch.insert("y_mm".into(), json!(b[1] - pad_y));
            patch.insert("h_mm".into(), json!(b[3] + 2.0 * pad_y));
        }
        apply_as_agent(
            &mut h,
            &ctx.turn,
            vec![Op::UpdateLayer {
                id: id.clone(),
                patch: Value::Object(patch),
            }],
        )?;
        let l = layer_by_id(&h.document, &id)?;
        let placed = [l.x_mm, l.y_mm, l.w_mm, l.h_mm];
        (id, b, placed)
    };
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    let res = edit_result(ctx, &id, &format!("레이어 {id} 상자 맞춤"))?;
    Ok(with_fields(
        res,
        json!({"layer_id": id, "box_mm": placed.map(r1), "content_mm": content.map(r1)}),
    ))
}

#[cfg(test)]
mod tests {
    use hoist_core::model::{Actor, Layer, TextProps};

    use super::*;
    use crate::agent::tools::tests::{rect_layer, test_ctx};
    use crate::agent::tools::{detail_for, execute_sync, label_for};

    #[test]
    fn arrange_row_and_fit_box_place_layers_by_their_ink() {
        let ctx = test_ctx(std::env::temp_dir().join("hoist-no-models"));
        let sized = |id: &str, x: f64, w: f64| Layer {
            w_mm: w,
            ..rect_layer(id, x)
        };
        let mut words = rect_layer("t", 0.0);
        words.kind = LayerKind::Text(TextProps {
            text: "가나다 라마바".into(),
            font_family: "Pretendard".into(),
            font_size_mm: 30.0,
            ..Default::default()
        });
        words.w_mm = 80.0;
        let mut pill = sized("L9", 0.0, 10.0);
        pill.name = "알약".into();
        let mut pinned = sized("pin", 0.0, 10.0);
        pinned.pinned = vec!["x_mm".into()];
        let ops = [
            sized("a", 0.0, 100.0),
            sized("b", 200.0, 50.0),
            sized("c", 210.0, 30.0),
            words,
            pill,
            pinned,
        ]
        .into_iter()
        .map(|l| Op::AddLayer {
            layer: Box::new(l),
            index: None,
        })
        .collect();
        lock(&ctx.handle).apply_ops(ops, Actor::User, None).expect("seed");
        let x = |id: &str| lock(&ctx.handle).document.layer(id).expect("layer").x_mm;
        let run = |name: &str, args: Value| -> Value {
            serde_json::from_str(&execute_sync(&ctx, name, args).expect(name).text).expect("json")
        };

        let v = run(
            "arrange_row",
            json!({"items": [["a"], ["b", "c"]], "x0_mm": 100, "x1_mm": 500, "justify": "center", "gap_mm": 20}),
        );
        assert_eq!((x("a"), x("b"), x("c")), (215.0, 335.0, 345.0));
        assert_eq!(v["gap_centers_mm"], json!([325.0]), "{v}");
        assert_eq!(
            (v["content_mm"].as_f64(), v["free_mm"].as_f64()),
            (Some(150.0), Some(230.0))
        );
        assert_eq!(v["row_mm"], json!([215.0, 385.0]));
        assert_eq!(v["items"][1]["layers"], json!(["b", "c"]));
        assert!(v.get("overflow").is_none() && v["region_mm"].is_array(), "{v}");

        run(
            "arrange_row",
            json!({"items": [["a"], ["b", "c"]], "x0_mm": 0, "x1_mm": 600, "justify": "space_between", "gap_mm": null}),
        );
        assert_eq!((x("a"), x("b"), x("c")), (0.0, 550.0, 560.0));
        run(
            "arrange_row",
            json!({"items": [["b"]], "x0_mm": 0, "x1_mm": 600, "justify": "space_between", "gap_mm": null}),
        );
        assert_eq!(x("b"), 275.0, "a single item is centered");
        run(
            "arrange_row",
            json!({"items": [["a"]], "x0_mm": 0, "x1_mm": 600, "justify": "end", "gap_mm": 5}),
        );
        assert_eq!(x("a"), 500.0);
        let v = run(
            "arrange_row",
            json!({"items": [["a"], ["b"]], "x0_mm": 0, "x1_mm": 100, "justify": "start", "gap_mm": 20}),
        );
        assert!(
            v["overflow"]
                .as_str()
                .is_some_and(|o| o.starts_with("content is 70 mm wider")),
            "{v}"
        );

        let ink = ink_mm(&ctx.fonts, lock(&ctx.handle).document.layer("t").expect("t")).0;
        let v = run(
            "fit_box",
            json!({"id": "알약", "around": ["t"], "pad_x_mm": 10, "pad_y_mm": 5, "axis": "both"}),
        );
        assert_eq!(v["layer_id"], "L9");
        assert_eq!(
            v["box_mm"],
            json!([ink[0] - 10.0, ink[1] - 5.0, ink[2] + 20.0, ink[3] + 10.0].map(r1))
        );
        run(
            "fit_box",
            json!({"id": "L9", "around": ["a"], "pad_x_mm": 0, "pad_y_mm": 0, "axis": "x"}),
        );
        let l = lock(&ctx.handle).document.layer("L9").expect("pill").clone();
        assert_eq!((l.x_mm, l.w_mm), (x("a"), 100.0));
        assert!((l.y_mm - (ink[1] - 5.0)).abs() < 1e-9, "axis x keeps y");

        let v = run("update_layer", json!({"id": "t", "patch": {"x_mm": 12}}));
        assert_eq!(v["lines"], json!(2), "{v}");
        assert!(v["ink_mm"].as_array().is_some_and(|a| a.len() == 4), "{v}");

        let err = |name: &str, args: Value| execute_sync(&ctx, name, args).err().expect(name).to_string();
        assert!(
            err(
                "fit_box",
                json!({"id": "t", "around": ["a"], "pad_x_mm": 0, "pad_y_mm": 0, "axis": "both"})
            )
            .contains("not a shape")
        );
        assert!(
            err(
                "arrange_row",
                json!({"items": [["없음"]], "x0_mm": 0, "x1_mm": 9, "justify": "center", "gap_mm": 0})
            )
            .contains("'없음' not found")
        );
        assert!(
            err(
                "arrange_row",
                json!({"items": [["pin"]], "x0_mm": 0, "x1_mm": 600, "justify": "center", "gap_mm": 0})
            )
            .contains("pinned")
        );
        assert_eq!(ctx.state.edits.load(Ordering::SeqCst), 8);
        let (doc, call) = (
            lock(&ctx.handle).document.clone(),
            json!({"items": [["제목 1", "x"], ["제목 2"]]}),
        );
        assert_eq!(label_for("arrange_row", &call), "줄 맞추기");
        assert_eq!(
            detail_for("arrange_row", &call, &doc, None).as_deref(),
            Some("제목 1, 제목 2")
        );
        let patch = json!({"id": "제목 1", "patch": {"x_mm": 10, "y_mm": 20, "w_mm": 300, "strategy": "text", "kind": {"font_family": "Pretendard"}}});
        assert_eq!(
            detail_for("update_layer", &patch, &doc, None).as_deref(),
            Some("제목 1 · 위치·크기·글꼴"),
            "the app's words, never x_mm"
        );
    }
}
