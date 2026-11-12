use hoist_core::model::{Document, Layer, LayerKind, TextAlign};
use hoist_core::ops::merge_patch;
use hoist_core::text::Fonts;
use serde_json::{Value, json};

pub fn ink_mm(fonts: &Fonts, l: &Layer) -> ([f64; 4], usize) {
    let own = [l.x_mm, l.y_mm, l.w_mm, l.h_mm];
    let LayerKind::Text(t) = &l.kind else {
        return (own, 0);
    };
    match fonts.layout_to_path(t, l.w_mm, l.h_mm) {
        Ok(run) if run.bbox_mm[2] > 0.0 && run.bbox_mm[3] > 0.0 => (
            [
                l.x_mm + run.bbox_mm[0],
                l.y_mm + run.bbox_mm[1],
                run.bbox_mm[2],
                run.bbox_mm[3],
            ],
            run.lines,
        ),
        Ok(run) => (own, run.lines),
        Err(_) => (own, 0),
    }
}

pub fn union_mm(boxes: impl IntoIterator<Item = [f64; 4]>) -> Option<[f64; 4]> {
    boxes
        .into_iter()
        .map(|b| [b[0], b[1], b[0] + b[2], b[1] + b[3]])
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
        .map(|u| [u[0], u[1], u[2] - u[0], u[3] - u[1]])
}

/// A loose text box makes the selection frame look wrong. None when hugging would re-wrap or shift the glyphs.
pub(super) fn hug_text_box(fonts: &Fonts, layer: &Layer) -> Option<[f64; 3]> {
    let LayerKind::Text(t) = &layer.kind else {
        return None;
    };
    if t.vertical || t.arc_bend != 0.0 || layer.rotation_deg != 0.0 {
        return None;
    }
    let (ink, lines) = ink_mm(fonts, layer);
    if lines == 0 {
        return None;
    }
    let pad = t.font_size_mm * 0.06;
    let (x, w) = match t.align {
        TextAlign::Left => (layer.x_mm, ink[0] + ink[2] - layer.x_mm + pad),
        TextAlign::Right => (ink[0] - pad, layer.x_mm + layer.w_mm - ink[0] + pad),
        TextAlign::Center => {
            let center = layer.x_mm + layer.w_mm / 2.0;
            let half = (center - ink[0]).max(ink[0] + ink[2] - center) + pad;
            (center - half, 2.0 * half)
        }
    };
    let h = lines as f64 * t.line_height * t.font_size_mm;
    if (w - layer.w_mm).abs() < 0.5 && (h - layer.h_mm).abs() < 0.5 {
        return None;
    }
    let mut fitted = layer.clone();
    (fitted.x_mm, fitted.w_mm, fitted.h_mm) = (x, w, h);
    let (after, after_lines) = ink_mm(fonts, &fitted);
    let same = after_lines == lines && (after[0] - ink[0]).abs() < 0.5 && (after[1] - ink[1]).abs() < 0.5;
    same.then_some([x, w, h])
}

/// A hugged one-line box would wrap new wording, so it is opened to the artboard width first; `hug_text_box` cuts it back.
pub(super) fn keep_one_line(fonts: &Fonts, layer: &Layer, artboard_w: f64, patch: &mut Value) {
    let LayerKind::Text(t) = &layer.kind else {
        return;
    };
    if patch.get("kind").is_none() || patch.get("w_mm").is_some() || ink_mm(fonts, layer).1 > 1 {
        return;
    }
    let x = patch["x_mm"].as_f64().unwrap_or(layer.x_mm);
    let wide = artboard_w.max(layer.w_mm);
    let opened = match t.align {
        TextAlign::Left => x,
        TextAlign::Center => x + (layer.w_mm - wide) / 2.0,
        TextAlign::Right => x + layer.w_mm - wide,
    };
    patch["x_mm"] = json!(opened);
    patch["w_mm"] = json!(wide);
}

/// Folded into the user's own patch so one undo takes it all back.
pub fn fit_typed_text(fonts: &Fonts, doc: &Document, patch: &mut Value, id: &str) {
    let Some(layer) = doc.layer(id) else {
        return;
    };
    if !patch["kind"]["text"].is_string() {
        return;
    }
    let mut opened = patch.clone();
    keep_one_line(fonts, layer, doc.artboard.width_mm, &mut opened);
    let Ok(mut merged) = serde_json::to_value(layer) else {
        return;
    };
    merge_patch(&mut merged, &opened);
    let Ok(typed) = serde_json::from_value::<Layer>(merged) else {
        return;
    };
    if let Some([x, w, h]) = hug_text_box(fonts, &typed) {
        patch["x_mm"] = json!(x);
        patch["w_mm"] = json!(w);
        patch["h_mm"] = json!(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::layer_args;

    #[test]
    fn typed_text_stays_on_one_line_in_a_hugged_box() {
        let fonts = Fonts::new(Some(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"),
        ));
        let raw = json!({"name": "제목", "kind": {"type": "text", "text": "청전동", "font_family": "Pretendard", "font_weight": 700, "font_size_mm": 100, "line_height": 1.2, "align": "left"}, "x_mm": 100, "y_mm": 50, "w_mm": 900, "h_mm": 300});
        let mut layer = layer_args::parse_layer(&raw, (6000.0, 900.0)).expect("text layer");
        let [x, w, h] = hug_text_box(&fonts, &layer).expect("cut");
        (layer.x_mm, layer.w_mm, layer.h_mm) = (x, w, h);
        let id = layer.id.clone();
        let mut doc = Document::new(6000.0, 900.0);
        doc.layers.push(layer.clone());

        let mut patch = json!({"kind": {"text": "청전동 주민센터 앞마당"}});
        fit_typed_text(&fonts, &doc, &mut patch, &id);
        assert!(patch["w_mm"].as_f64().expect("width") > layer.w_mm, "{patch}");
        assert_eq!(patch["x_mm"].as_f64(), Some(layer.x_mm));
        assert!(
            (patch["h_mm"].as_f64().expect("height") - layer.h_mm).abs() < 0.01,
            "stays one line"
        );

        let mut color = json!({"opacity": 0.5});
        fit_typed_text(&fonts, &doc, &mut color, &id);
        assert_eq!(color, json!({"opacity": 0.5}));
    }

    #[test]
    fn a_text_box_hugs_its_glyphs_without_moving_them() {
        let fonts = Fonts::new(Some(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"),
        ));
        let text = |align: &str, body: &str, w: f64| {
            let raw = json!({"name": "제목", "kind": {"type": "text", "text": body, "font_family": "Pretendard", "font_weight": 700, "font_size_mm": 100, "line_height": 1.2, "align": align}, "x_mm": 100, "y_mm": 50, "w_mm": w, "h_mm": 300});
            layer_args::parse_layer(&raw, (6000.0, 900.0)).expect("text layer")
        };
        for align in ["left", "center", "right"] {
            let loose = text(align, "청전동", 900.0);
            let (ink, _) = ink_mm(&fonts, &loose);
            let [x, w, h] = hug_text_box(&fonts, &loose).expect("a loose box is cut");
            assert!(
                w < ink[2] + 20.0 && w >= ink[2],
                "{align}: {w} around ink {}",
                ink[2]
            );
            assert_eq!(h, 120.0, "one line box");
            let mut tight = loose.clone();
            (tight.x_mm, tight.w_mm, tight.h_mm) = (x, w, h);
            let (after, lines) = ink_mm(&fonts, &tight);
            assert!(
                (after[0] - ink[0]).abs() < 0.5 && lines == 1,
                "{align}: glyphs stay put"
            );
            assert!(hug_text_box(&fonts, &tight).is_none(), "{align}: already fits");
        }
        let wrapped = text("left", "가나다 라마바 사아자", 420.0);
        let lines = ink_mm(&fonts, &wrapped).1;
        assert!(lines > 1, "wraps at the box width");
        let [_, w, h] = hug_text_box(&fonts, &wrapped).expect("cut to the widest line");
        assert!(
            w <= 427.0 && (h - lines as f64 * 120.0).abs() < 0.01,
            "{w} × {h} for {lines} lines"
        );

        let tight = {
            let mut l = text("center", "청전동", 900.0);
            let [x, w, h] = hug_text_box(&fonts, &l).expect("cut");
            (l.x_mm, l.w_mm, l.h_mm) = (x, w, h);
            l
        };
        let mut longer = json!({"kind": {"text": "청전동 주민역량강화 참여자"}});
        keep_one_line(&fonts, &tight, 6000.0, &mut longer);
        assert_eq!(
            longer["w_mm"],
            json!(6000.0),
            "new wording is laid out on one line first"
        );
        let center = tight.x_mm + tight.w_mm / 2.0;
        assert_eq!(
            longer["x_mm"].as_f64().map(|x| x + 3000.0),
            Some(center),
            "around the same centre"
        );
        let mut moved = json!({"x_mm": 10});
        keep_one_line(&fonts, &tight, 6000.0, &mut moved);
        assert_eq!(moved, json!({"x_mm": 10}), "a move keeps the box");
    }
}
