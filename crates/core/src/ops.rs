use serde_json::{Map, Value};

use crate::model::*;
use crate::{CoreError, Result};

mod wording;

pub use wording::{describe, field_label};

/// Deleting a layer also removes its descendants, but the returned inverse re-adds only that layer.
/// `ProjectHandle::apply_ops` expands a delete into one delete per descendant (deepest first), so every
/// removed layer gets its own `OpEntry` and undo restores the whole subtree.
pub fn apply_op(doc: &mut Document, op: &Op) -> Result<Op> {
    match op {
        Op::AddLayer { layer, index } => add_layer(doc, layer, *index),
        Op::DeleteLayer { id } => delete_layer(doc, id),
        Op::UpdateLayer { id, patch } => update_layer(doc, id, patch),
        Op::MoveLayer { id, parent, index } => move_layer(doc, id, parent.as_deref(), *index),
        Op::SetArtboard { patch } => set_artboard(doc, patch),
    }
}

fn add_layer(doc: &mut Document, layer: &Layer, index: Option<usize>) -> Result<Op> {
    if layer.id.is_empty() || doc.layer(&layer.id).is_some() {
        return Err(CoreError::invalid(format!(
            "layer id {:?} is empty or already used",
            layer.id
        )));
    }
    validate(doc, layer)?;
    let at = insert_pos(&doc.layers, layer.parent.as_deref(), index);
    doc.layers.insert(at, layer.clone());
    Ok(Op::DeleteLayer { id: layer.id.clone() })
}

fn delete_layer(doc: &mut Document, id: &str) -> Result<Op> {
    let layer = doc.layers[position(&doc.layers, id)?].clone();
    let index = sibling_index(doc, &layer);
    let ids = subtree(&doc.layers, id);
    doc.layers.retain(|l| !ids.contains(&l.id));
    Ok(Op::AddLayer {
        layer: Box::new(layer),
        index: Some(index),
    })
}

fn update_layer(doc: &mut Document, id: &str, patch: &Value) -> Result<Op> {
    if !patch.is_object() {
        return Err(CoreError::invalid("update_layer patch must be an object"));
    }
    let i = position(&doc.layers, id)?;
    let old = serde_json::to_value(&doc.layers[i])?;
    let mut merged = old.clone();
    merge_patch(&mut merged, patch);
    let layer: Layer =
        serde_json::from_value(merged).map_err(|e| CoreError::invalid(format!("{id}: {e}")))?;
    if layer.id != id {
        return Err(CoreError::invalid(format!("{id}: id cannot change")));
    }
    if layer.parent != doc.layers[i].parent {
        return Err(CoreError::invalid(format!(
            "{id}: use move_layer to change parent"
        )));
    }
    let has_children = doc.layers.iter().any(|l| l.parent.as_deref() == Some(id));
    if has_children && !matches!(layer.kind, LayerKind::Group) {
        return Err(CoreError::invalid(format!(
            "{id}: a layer with children must stay a group"
        )));
    }
    validate(doc, &layer)?;
    let inverse = diff(&serde_json::to_value(&layer)?, &old);
    doc.layers[i] = layer;
    Ok(Op::UpdateLayer {
        id: id.to_string(),
        patch: inverse,
    })
}

fn move_layer(doc: &mut Document, id: &str, parent: Option<&str>, index: usize) -> Result<Op> {
    let i = position(&doc.layers, id)?;
    let ids = subtree(&doc.layers, id);
    if let Some(p) = parent {
        if ids.iter().any(|x| x == p) {
            return Err(CoreError::invalid(format!(
                "{id}: cannot move into itself or a descendant"
            )));
        }
        check_parent(doc, p)?;
    }
    let inverse = Op::MoveLayer {
        id: id.to_string(),
        parent: doc.layers[i].parent.clone(),
        index: sibling_index(doc, &doc.layers[i]),
    };
    let (mut block, mut rest): (Vec<Layer>, Vec<Layer>) = std::mem::take(&mut doc.layers)
        .into_iter()
        .partition(|l| ids.contains(&l.id));
    if let Some(root) = block.iter_mut().find(|l| l.id == id) {
        root.parent = parent.map(String::from);
    }
    let at = insert_pos(&rest, parent, Some(index));
    rest.splice(at..at, block);
    doc.layers = rest;
    Ok(inverse)
}

fn set_artboard(doc: &mut Document, patch: &Value) -> Result<Op> {
    if !patch.is_object() {
        return Err(CoreError::invalid("set_artboard patch must be an object"));
    }
    let old = serde_json::to_value(&doc.artboard)?;
    let mut merged = old.clone();
    merge_patch(&mut merged, patch);
    let artboard: Artboard =
        serde_json::from_value(merged).map_err(|e| CoreError::invalid(format!("artboard: {e}")))?;
    if !(artboard.width_mm > 0.0 && artboard.height_mm > 0.0) {
        return Err(CoreError::invalid("artboard: width_mm and height_mm must be > 0"));
    }
    if artboard.bleed_mm.is_nan() || artboard.bleed_mm < 0.0 {
        return Err(CoreError::invalid("artboard: bleed_mm must be >= 0"));
    }
    let inverse = diff(&serde_json::to_value(&artboard)?, &old);
    doc.artboard = artboard;
    Ok(Op::SetArtboard { patch: inverse })
}

fn validate(doc: &Document, layer: &Layer) -> Result<()> {
    if !(layer.w_mm > 0.0 && layer.h_mm > 0.0) {
        return Err(CoreError::invalid(format!(
            "{}: w_mm and h_mm must be > 0",
            layer.id
        )));
    }
    if !(0.0..=1.0).contains(&layer.opacity) {
        return Err(CoreError::invalid(format!(
            "{}: opacity must be within 0..1",
            layer.id
        )));
    }
    if let Some(problem) = kind_problem(&layer.kind) {
        return Err(CoreError::invalid(format!("{}: {problem}", layer.id)));
    }
    match &layer.parent {
        Some(p) => check_parent(doc, p),
        None => Ok(()),
    }
}

fn kind_problem(kind: &LayerKind) -> Option<&'static str> {
    let within = |v: f64, lo: f64, hi: f64| (lo..=hi).contains(&v);
    match kind {
        LayerKind::Text(t) if !within(t.scale_x, 0.5, 1.5) => Some("scale_x must be within 0.5..1.5"),
        LayerKind::Text(t) if !within(t.skew_deg, -60.0, 60.0) => Some("skew_deg must be within -60..60"),
        LayerKind::Text(t) if !within(t.arc_bend, -1.0, 1.0) => Some("arc_bend must be within -1..1"),
        LayerKind::Shape(ShapeProps::Polygon { sides }) if !(3..=64).contains(sides) => {
            Some("polygon sides must be within 3..64")
        }
        LayerKind::Shape(ShapeProps::Star { points, .. }) if !(3..=64).contains(points) => {
            Some("star points must be within 3..64")
        }
        LayerKind::Shape(ShapeProps::Star { inner_ratio, .. }) if !within(*inner_ratio, 0.05, 0.95) => {
            Some("star inner_ratio must be within 0.05..0.95")
        }
        LayerKind::Image(i)
            if ![i.adjust.brightness, i.adjust.contrast, i.adjust.saturation]
                .iter()
                .all(|&v| within(v, -1.0, 1.0)) =>
        {
            Some("image adjust values must be within -1..1")
        }
        LayerKind::Image(i) => match i.crop_px {
            Some([x, y, w, h])
                if w == 0
                    || h == 0
                    || u64::from(x) + u64::from(w) > u64::from(i.natural_w_px)
                    || u64::from(y) + u64::from(h) > u64::from(i.natural_h_px) =>
            {
                Some("crop_px must be [x, y, w, h] with w, h > 0 inside the natural image size")
            }
            _ => None,
        },
        _ => None,
    }
}

fn check_parent(doc: &Document, parent: &str) -> Result<()> {
    match doc.layer(parent) {
        None => Err(CoreError::LayerNotFound(parent.to_string())),
        Some(p) if !matches!(p.kind, LayerKind::Group) => {
            Err(CoreError::invalid(format!("parent {parent} is not a group")))
        }
        Some(_) => Ok(()),
    }
}

fn position(layers: &[Layer], id: &str) -> Result<usize> {
    layers
        .iter()
        .position(|l| l.id == id)
        .ok_or_else(|| CoreError::LayerNotFound(id.to_string()))
}

fn insert_pos(layers: &[Layer], parent: Option<&str>, index: Option<usize>) -> usize {
    let siblings: Vec<usize> = layers
        .iter()
        .enumerate()
        .filter(|(_, l)| l.parent.as_deref() == parent)
        .map(|(i, _)| i)
        .collect();
    if let Some(&at) = index.and_then(|i| siblings.get(i)) {
        return at;
    }
    let anchor = siblings
        .last()
        .copied()
        .or_else(|| parent.and_then(|p| layers.iter().position(|l| l.id == p)));
    match anchor {
        Some(a) => {
            let ids = subtree(layers, &layers[a].id);
            layers
                .iter()
                .rposition(|l| ids.contains(&l.id))
                .map_or(a, |p| p + 1)
        }
        None => layers.len(),
    }
}

pub(crate) fn sibling_index(doc: &Document, layer: &Layer) -> usize {
    doc.layers
        .iter()
        .filter(|l| l.parent == layer.parent)
        .position(|l| l.id == layer.id)
        .unwrap_or(0)
}

pub(crate) fn subtree(layers: &[Layer], id: &str) -> Vec<LayerId> {
    let mut ids = vec![id.to_string()];
    let mut i = 0;
    while i < ids.len() {
        let children: Vec<LayerId> = layers
            .iter()
            .filter(|l| l.parent.as_deref() == Some(ids[i].as_str()))
            .map(|l| l.id.clone())
            .collect();
        ids.extend(children);
        i += 1;
    }
    ids
}

pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    if let Value::Object(map) = target {
        for (key, value) in patch {
            if value.is_null() {
                map.remove(key);
            } else {
                merge_patch(map.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    }
}

pub fn patch_paths(patch: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_paths("", patch, &mut out);
    out
}

fn collect_paths(prefix: &str, value: &Value, out: &mut Vec<String>) {
    match value.as_object() {
        Some(map) if !map.is_empty() => {
            for (key, value) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                collect_paths(&path, value, out);
            }
        }
        _ if !prefix.is_empty() => out.push(prefix.to_string()),
        _ => {}
    }
}

pub(crate) fn diff(from: &Value, to: &Value) -> Value {
    let (Value::Object(from), Value::Object(to)) = (from, to) else {
        return to.clone();
    };
    let mut out = Map::new();
    for (key, new) in to {
        match from.get(key) {
            Some(old) if old == new => {}
            Some(old) if old.is_object() && new.is_object() => {
                out.insert(key.clone(), diff(old, new));
            }
            _ => {
                out.insert(key.clone(), new.clone());
            }
        }
    }
    for key in from.keys().filter(|k| !to.contains_key(*k)) {
        out.insert(key.clone(), Value::Null);
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{layer, sample, text};
    use serde_json::json;

    fn ids(doc: &Document) -> Vec<&str> {
        doc.layers.iter().map(|l| l.id.as_str()).collect()
    }

    fn round_trip(doc: &Document, op: Op) -> Document {
        let mut work = doc.clone();
        let inverse = apply_op(&mut work, &op).expect("apply");
        let changed = work.clone();
        apply_op(&mut work, &inverse).expect("inverse");
        assert_eq!(work, *doc, "inverse of {op:?} did not restore");
        changed
    }

    #[test]
    fn add_layer_respects_sibling_index_and_round_trips() {
        let doc = sample();
        assert_eq!(ids(&doc), ["L1", "L2", "L3", "L4", "L5", "L6"]);
        let changed = round_trip(
            &doc,
            Op::AddLayer {
                layer: Box::new(layer("L7", "새", Some("L2"), text("x"))),
                index: Some(1),
            },
        );
        assert_eq!(ids(&changed), ["L1", "L2", "L3", "L7", "L4", "L5", "L6"]);
        let changed = round_trip(
            &doc,
            Op::AddLayer {
                layer: Box::new(layer("L7", "새", Some("L2"), text("x"))),
                index: None,
            },
        );
        assert_eq!(ids(&changed), ["L1", "L2", "L3", "L4", "L5", "L7", "L6"]);
        let changed = round_trip(
            &doc,
            Op::AddLayer {
                layer: Box::new(layer("L7", "새", None, text("x"))),
                index: Some(0),
            },
        );
        assert_eq!(ids(&changed), ["L7", "L1", "L2", "L3", "L4", "L5", "L6"]);
    }

    #[test]
    fn delete_leaf_round_trips() {
        let doc = sample();
        let changed = round_trip(&doc, Op::DeleteLayer { id: "L3".into() });
        assert!(changed.layer("L3").is_none());
    }

    #[test]
    fn delete_group_removes_descendants() {
        let mut doc = sample();
        let inverse = apply_op(&mut doc, &Op::DeleteLayer { id: "L2".into() }).expect("delete");
        assert_eq!(ids(&doc), ["L1", "L6"]);
        assert!(matches!(inverse, Op::AddLayer { ref layer, index: Some(1) } if layer.id == "L2"));
    }

    #[test]
    fn update_layer_round_trips_including_kind_switch() {
        let doc = sample();
        let changed = round_trip(
            &doc,
            Op::UpdateLayer {
                id: "L3".into(),
                patch: json!({"x_mm": 130, "kind": {"text": "청전동 주민"}}),
            },
        );
        let l3 = changed.layer("L3").expect("L3");
        assert_eq!(l3.x_mm, 130.0);
        assert!(matches!(&l3.kind, LayerKind::Text(t) if t.text == "청전동 주민"));
        round_trip(
            &doc,
            Op::UpdateLayer {
                id: "L6".into(),
                patch: json!({"kind": {"type": "shape", "shape": "rect"}}),
            },
        );
        round_trip(
            &doc,
            Op::UpdateLayer {
                id: "L6".into(),
                patch: json!({"source": {"asset": "a.png"}, "opacity": null}),
            },
        );
    }

    #[test]
    fn new_text_and_shape_fields_patch_and_range_check() {
        let doc = sample();
        let changed = round_trip(
            &doc,
            Op::UpdateLayer {
                id: "L6".into(),
                patch: json!({"kind": {"scale_x": 0.8, "skew_deg": 12, "vertical": true, "arc_bend": -0.4}}),
            },
        );
        let LayerKind::Text(t) = &changed.layer("L6").expect("L6").kind else {
            panic!("L6 stays text");
        };
        assert_eq!(
            (t.scale_x, t.skew_deg, t.vertical, t.arc_bend),
            (0.8, 12.0, true, -0.4)
        );
        let changed = round_trip(
            &doc,
            Op::UpdateLayer {
                id: "L5".into(),
                patch: json!({"kind": {"shape": "star", "points": 5, "inner_ratio": 0.4}}),
            },
        );
        assert!(matches!(
            changed.layer("L5").expect("L5").kind,
            LayerKind::Shape(ShapeProps::Star { points: 5, .. })
        ));
        for (id, patch) in [
            ("L6", json!({"kind": {"scale_x": 2.0}})),
            ("L6", json!({"kind": {"arc_bend": 1.5}})),
            ("L5", json!({"kind": {"shape": "polygon", "sides": 2}})),
            (
                "L5",
                json!({"kind": {"shape": "star", "points": 5, "inner_ratio": 1.0}}),
            ),
        ] {
            let mut work = doc.clone();
            let err = apply_op(
                &mut work,
                &Op::UpdateLayer {
                    id: id.into(),
                    patch: patch.clone(),
                },
            );
            assert!(err.is_err(), "{patch} should be rejected");
            assert_eq!(work, doc);
        }
    }

    #[test]
    fn update_layer_validates() {
        let mut doc = sample();
        let bad = [
            ("L3", json!({"w_mm": 0})),
            ("L3", json!({"opacity": 1.5})),
            ("L3", json!({"id": "L9"})),
            ("L3", json!({"parent": null})),
            ("L3", json!({"x_mm": null})),
            ("L2", json!({"kind": {"type": "shape", "shape": "ellipse"}})),
        ];
        for (id, patch) in bad {
            let r = apply_op(
                &mut doc,
                &Op::UpdateLayer {
                    id: id.into(),
                    patch: patch.clone(),
                },
            );
            assert!(
                matches!(r, Err(CoreError::InvalidOp(_))),
                "{patch} should be rejected"
            );
        }
        let r = apply_op(
            &mut doc,
            &Op::UpdateLayer {
                id: "nope".into(),
                patch: json!({}),
            },
        );
        assert!(matches!(r, Err(CoreError::LayerNotFound(_))));
        assert_eq!(doc, sample());
    }

    #[test]
    fn move_layer_round_trips_and_rejects_cycles() {
        let doc = sample();
        let changed = round_trip(
            &doc,
            Op::MoveLayer {
                id: "L4".into(),
                parent: None,
                index: 0,
            },
        );
        assert_eq!(ids(&changed), ["L4", "L5", "L1", "L2", "L3", "L6"]);
        assert_eq!(changed.layer("L4").and_then(|l| l.parent.clone()), None);
        let changed = round_trip(
            &doc,
            Op::MoveLayer {
                id: "L6".into(),
                parent: Some("L4".into()),
                index: 0,
            },
        );
        assert_eq!(ids(&changed), ["L1", "L2", "L3", "L4", "L6", "L5"]);
        round_trip(
            &doc,
            Op::MoveLayer {
                id: "L3".into(),
                parent: Some("L2".into()),
                index: 5,
            },
        );

        let mut work = doc.clone();
        for (id, parent) in [("L2", "L5"), ("L2", "L4"), ("L2", "L2")] {
            let r = apply_op(
                &mut work,
                &Op::MoveLayer {
                    id: id.into(),
                    parent: Some(parent.into()),
                    index: 0,
                },
            );
            assert!(r.is_err(), "{id} into {parent}");
        }
        let r = apply_op(
            &mut work,
            &Op::MoveLayer {
                id: "L1".into(),
                parent: Some("L3".into()),
                index: 0,
            },
        );
        assert!(matches!(r, Err(CoreError::InvalidOp(_))));
        assert_eq!(work, doc);
    }

    #[test]
    fn set_artboard_round_trips_and_validates() {
        let doc = sample();
        let changed = round_trip(
            &doc,
            Op::SetArtboard {
                patch: json!({"width_mm": 1200, "background": null}),
            },
        );
        assert_eq!(changed.artboard.width_mm, 1200.0);
        assert_eq!(changed.artboard.background, None);
        let mut work = doc.clone();
        assert!(
            apply_op(
                &mut work,
                &Op::SetArtboard {
                    patch: json!({"height_mm": -1})
                }
            )
            .is_err()
        );
    }

    #[test]
    fn merge_patch_follows_rfc7386() {
        let mut target = json!({"a": "b", "c": {"d": "e", "f": "g"}, "arr": [1, 2]});
        merge_patch(
            &mut target,
            &json!({"a": "z", "c": {"f": null}, "arr": [3], "n": {"m": 1}}),
        );
        assert_eq!(
            target,
            json!({"a": "z", "c": {"d": "e"}, "arr": [3], "n": {"m": 1}})
        );
        let mut scalar = json!([1]);
        merge_patch(&mut scalar, &json!({"a": null, "b": 2}));
        assert_eq!(scalar, json!({"b": 2}));
    }

    #[test]
    fn patch_paths_lists_leaves() {
        let mut paths = patch_paths(
            &json!({"x_mm": 1, "kind": {"text": "a", "font_size_mm": 3}, "strokes": [{"width_mm": 1}]}),
        );
        paths.sort();
        assert_eq!(paths, ["kind.font_size_mm", "kind.text", "strokes", "x_mm"]);
    }
}
