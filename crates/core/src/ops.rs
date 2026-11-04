use serde_json::{Map, Value};

use crate::model::*;
use crate::{CoreError, Result};

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
                let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                collect_paths(&path, value, out);
            }
        }
        _ if !prefix.is_empty() => out.push(prefix.to_string()),
        _ => {}
    }
}

/// `doc` is the document before `op` is applied.
pub fn describe(op: &Op, doc: &Document) -> String {
    let name = |id: &str| doc.layer(id).map_or_else(|| id.to_string(), |l| l.name.clone());
    match op {
        Op::AddLayer { layer, .. } => format!("레이어 추가 · {}", layer.name),
        Op::DeleteLayer { id } => format!("레이어 삭제 · {}", name(id)),
        Op::MoveLayer { id, parent: Some(p), .. } => format!("레이어 이동 · {} → {}", name(id), name(p)),
        Op::MoveLayer { id, parent: None, .. } => format!("레이어 이동 · {}", name(id)),
        Op::UpdateLayer { id, patch } => {
            let old = doc.layer(id).and_then(|l| serde_json::to_value(l).ok()).unwrap_or(Value::Null);
            format!("{} · {}", name(id), changes(&old, patch))
        }
        Op::SetArtboard { patch } => {
            let old = serde_json::to_value(&doc.artboard).unwrap_or(Value::Null);
            format!("아트보드 · {}", changes(&old, patch))
        }
    }
}

fn changes(old: &Value, patch: &Value) -> String {
    let mut paths = patch_paths(patch);
    if paths.len() > 1 {
        paths.retain(|p| p != "pinned");
    }
    let mut parts: Vec<String> = paths
        .iter()
        .take(3)
        .map(|path| {
            let pointer = format!("/{}", path.replace('.', "/"));
            format!("{path} {} → {}", show(old.pointer(&pointer)), show(patch.pointer(&pointer)))
        })
        .collect();
    if paths.len() > 3 {
        parts.push(format!("외 {}개", paths.len() - 3));
    }
    parts.join(", ")
}

fn show(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "없음".into(),
        Some(Value::Number(n)) => {
            let v = n.as_f64().unwrap_or(0.0);
            let s = format!("{v:.2}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        Some(Value::String(s)) if s.chars().count() > 20 => format!("{}…", s.chars().take(20).collect::<String>()),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => format!("{}개", a.len()),
        Some(Value::Object(_)) => "{…}".into(),
        Some(Value::Bool(b)) => b.to_string(),
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

pub(crate) fn sibling_index(doc: &Document, layer: &Layer) -> usize {
    doc.layers
        .iter()
        .filter(|l| l.parent == layer.parent)
        .position(|l| l.id == layer.id)
        .unwrap_or(0)
}

fn invalid(msg: impl Into<String>) -> CoreError {
    CoreError::InvalidOp(msg.into())
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
            layers.iter().rposition(|l| ids.contains(&l.id)).map_or(a, |p| p + 1)
        }
        None => layers.len(),
    }
}

fn check_parent(doc: &Document, parent: &str) -> Result<()> {
    match doc.layer(parent) {
        None => Err(CoreError::LayerNotFound(parent.to_string())),
        Some(p) if !matches!(p.kind, LayerKind::Group) => Err(invalid(format!("parent {parent} is not a group"))),
        Some(_) => Ok(()),
    }
}

fn validate(doc: &Document, layer: &Layer) -> Result<()> {
    if !(layer.w_mm > 0.0 && layer.h_mm > 0.0) {
        return Err(invalid(format!("{}: w_mm and h_mm must be > 0", layer.id)));
    }
    if !(0.0..=1.0).contains(&layer.opacity) {
        return Err(invalid(format!("{}: opacity must be within 0..1", layer.id)));
    }
    match &layer.parent {
        Some(p) => check_parent(doc, p),
        None => Ok(()),
    }
}

fn add_layer(doc: &mut Document, layer: &Layer, index: Option<usize>) -> Result<Op> {
    if layer.id.is_empty() || doc.layer(&layer.id).is_some() {
        return Err(invalid(format!("layer id {:?} is empty or already used", layer.id)));
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
    Ok(Op::AddLayer { layer, index: Some(index) })
}

fn update_layer(doc: &mut Document, id: &str, patch: &Value) -> Result<Op> {
    if !patch.is_object() {
        return Err(invalid("update_layer patch must be an object"));
    }
    let i = position(&doc.layers, id)?;
    let old = serde_json::to_value(&doc.layers[i])?;
    let mut merged = old.clone();
    merge_patch(&mut merged, patch);
    let layer: Layer = serde_json::from_value(merged).map_err(|e| invalid(format!("{id}: {e}")))?;
    if layer.id != id {
        return Err(invalid(format!("{id}: id cannot change")));
    }
    if layer.parent != doc.layers[i].parent {
        return Err(invalid(format!("{id}: use move_layer to change parent")));
    }
    let has_children = doc.layers.iter().any(|l| l.parent.as_deref() == Some(id));
    if has_children && !matches!(layer.kind, LayerKind::Group) {
        return Err(invalid(format!("{id}: a layer with children must stay a group")));
    }
    validate(doc, &layer)?;
    let inverse = diff(&serde_json::to_value(&layer)?, &old);
    doc.layers[i] = layer;
    Ok(Op::UpdateLayer { id: id.to_string(), patch: inverse })
}

fn move_layer(doc: &mut Document, id: &str, parent: Option<&str>, index: usize) -> Result<Op> {
    let i = position(&doc.layers, id)?;
    let ids = subtree(&doc.layers, id);
    if let Some(p) = parent {
        if ids.iter().any(|x| x == p) {
            return Err(invalid(format!("{id}: cannot move into itself or a descendant")));
        }
        check_parent(doc, p)?;
    }
    let inverse = Op::MoveLayer {
        id: id.to_string(),
        parent: doc.layers[i].parent.clone(),
        index: sibling_index(doc, &doc.layers[i]),
    };
    let (mut block, mut rest): (Vec<Layer>, Vec<Layer>) =
        std::mem::take(&mut doc.layers).into_iter().partition(|l| ids.contains(&l.id));
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
        return Err(invalid("set_artboard patch must be an object"));
    }
    let old = serde_json::to_value(&doc.artboard)?;
    let mut merged = old.clone();
    merge_patch(&mut merged, patch);
    let artboard: Artboard = serde_json::from_value(merged).map_err(|e| invalid(format!("artboard: {e}")))?;
    if !(artboard.width_mm > 0.0 && artboard.height_mm > 0.0) {
        return Err(invalid("artboard: width_mm and height_mm must be > 0"));
    }
    if artboard.bleed_mm.is_nan() || artboard.bleed_mm < 0.0 {
        return Err(invalid("artboard: bleed_mm must be >= 0"));
    }
    let inverse = diff(&serde_json::to_value(&artboard)?, &old);
    doc.artboard = artboard;
    Ok(Op::SetArtboard { patch: inverse })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn layer(id: &str, name: &str, parent: Option<&str>, kind: LayerKind) -> Layer {
        Layer {
            id: id.into(),
            name: name.into(),
            parent: parent.map(String::from),
            kind,
            x_mm: 100.0,
            y_mm: 10.0,
            w_mm: 50.0,
            h_mm: 20.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![],
            strokes: vec![],
            effects: vec![],
            strategy: Strategy::NativeVector,
            strategy_reason: String::new(),
            pinned: vec![],
            source: None,
        }
    }

    pub(crate) fn text(t: &str) -> LayerKind {
        LayerKind::Text(TextProps {
            text: t.into(),
            font_family: "Pretendard".into(),
            font_weight: 700,
            font_size_mm: 10.0,
            letter_spacing_mm: 0.0,
            line_height: 1.2,
            align: TextAlign::Left,
        })
    }

    pub(crate) fn sample() -> Document {
        let mut doc = Document::new(900.0, 300.0);
        for op in [
            Op::AddLayer { layer: layer("L1", "배경", None, LayerKind::Shape(ShapeProps::Rect { rx_mm: 0.0 })), index: None },
            Op::AddLayer { layer: layer("L2", "정보 띠", None, LayerKind::Group), index: None },
            Op::AddLayer { layer: layer("L3", "청전동 주민역량강화", Some("L2"), text("청전동")), index: None },
            Op::AddLayer { layer: layer("L4", "문인화", Some("L2"), LayerKind::Group), index: None },
            Op::AddLayer { layer: layer("L5", "잎", Some("L4"), LayerKind::Shape(ShapeProps::Ellipse)), index: None },
            Op::AddLayer { layer: layer("L6", "제목", None, text("제목")), index: None },
        ] {
            apply_op(&mut doc, &op).expect("sample op");
        }
        doc
    }

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
        let changed = round_trip(&doc, Op::AddLayer { layer: layer("L7", "새", Some("L2"), text("x")), index: Some(1) });
        assert_eq!(ids(&changed), ["L1", "L2", "L3", "L7", "L4", "L5", "L6"]);
        let changed = round_trip(&doc, Op::AddLayer { layer: layer("L7", "새", Some("L2"), text("x")), index: None });
        assert_eq!(ids(&changed), ["L1", "L2", "L3", "L4", "L5", "L7", "L6"]);
        let changed = round_trip(&doc, Op::AddLayer { layer: layer("L7", "새", None, text("x")), index: Some(0) });
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
        let changed = round_trip(&doc, Op::UpdateLayer { id: "L3".into(), patch: json!({"x_mm": 130, "kind": {"text": "청전동 주민"}}) });
        let l3 = changed.layer("L3").expect("L3");
        assert_eq!(l3.x_mm, 130.0);
        assert!(matches!(&l3.kind, LayerKind::Text(t) if t.text == "청전동 주민"));
        round_trip(&doc, Op::UpdateLayer { id: "L6".into(), patch: json!({"kind": {"type": "shape", "shape": "rect"}}) });
        round_trip(&doc, Op::UpdateLayer { id: "L6".into(), patch: json!({"source": {"asset": "a.png"}, "opacity": null}) });
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
            let r = apply_op(&mut doc, &Op::UpdateLayer { id: id.into(), patch: patch.clone() });
            assert!(matches!(r, Err(CoreError::InvalidOp(_))), "{patch} should be rejected");
        }
        let r = apply_op(&mut doc, &Op::UpdateLayer { id: "nope".into(), patch: json!({}) });
        assert!(matches!(r, Err(CoreError::LayerNotFound(_))));
        assert_eq!(doc, sample());
    }

    #[test]
    fn move_layer_round_trips_and_rejects_cycles() {
        let doc = sample();
        let changed = round_trip(&doc, Op::MoveLayer { id: "L4".into(), parent: None, index: 0 });
        assert_eq!(ids(&changed), ["L4", "L5", "L1", "L2", "L3", "L6"]);
        assert_eq!(changed.layer("L4").and_then(|l| l.parent.clone()), None);
        let changed = round_trip(&doc, Op::MoveLayer { id: "L6".into(), parent: Some("L4".into()), index: 0 });
        assert_eq!(ids(&changed), ["L1", "L2", "L3", "L4", "L6", "L5"]);
        round_trip(&doc, Op::MoveLayer { id: "L3".into(), parent: Some("L2".into()), index: 5 });

        let mut work = doc.clone();
        for (id, parent) in [("L2", "L5"), ("L2", "L4"), ("L2", "L2")] {
            let r = apply_op(&mut work, &Op::MoveLayer { id: id.into(), parent: Some(parent.into()), index: 0 });
            assert!(r.is_err(), "{id} into {parent}");
        }
        let r = apply_op(&mut work, &Op::MoveLayer { id: "L1".into(), parent: Some("L3".into()), index: 0 });
        assert!(matches!(r, Err(CoreError::InvalidOp(_))));
        assert_eq!(work, doc);
    }

    #[test]
    fn set_artboard_round_trips_and_validates() {
        let doc = sample();
        let changed = round_trip(&doc, Op::SetArtboard { patch: json!({"width_mm": 1200, "background": null}) });
        assert_eq!(changed.artboard.width_mm, 1200.0);
        assert_eq!(changed.artboard.background, None);
        let mut work = doc.clone();
        assert!(apply_op(&mut work, &Op::SetArtboard { patch: json!({"height_mm": -1}) }).is_err());
    }

    #[test]
    fn merge_patch_follows_rfc7386() {
        let mut target = json!({"a": "b", "c": {"d": "e", "f": "g"}, "arr": [1, 2]});
        merge_patch(&mut target, &json!({"a": "z", "c": {"f": null}, "arr": [3], "n": {"m": 1}}));
        assert_eq!(target, json!({"a": "z", "c": {"d": "e"}, "arr": [3], "n": {"m": 1}}));
        let mut scalar = json!([1]);
        merge_patch(&mut scalar, &json!({"a": null, "b": 2}));
        assert_eq!(scalar, json!({"b": 2}));
    }

    #[test]
    fn patch_paths_lists_leaves() {
        let mut paths = patch_paths(&json!({"x_mm": 1, "kind": {"text": "a", "font_size_mm": 3}, "strokes": [{"width_mm": 1}]}));
        paths.sort();
        assert_eq!(paths, ["kind.font_size_mm", "kind.text", "strokes", "x_mm"]);
    }

    #[test]
    fn describe_summarizes_in_korean() {
        let doc = sample();
        assert_eq!(
            describe(&Op::UpdateLayer { id: "L3".into(), patch: json!({"x_mm": 130}) }, &doc),
            "청전동 주민역량강화 · x_mm 100 → 130"
        );
        assert_eq!(
            describe(&Op::AddLayer { layer: layer("L9", "정보 띠", None, LayerKind::Group), index: None }, &doc),
            "레이어 추가 · 정보 띠"
        );
        assert_eq!(describe(&Op::DeleteLayer { id: "L4".into() }, &doc), "레이어 삭제 · 문인화");
        assert_eq!(describe(&Op::SetArtboard { patch: json!({"width_mm": 1200.5}) }, &doc), "아트보드 · width_mm 900 → 1200.5");
    }
}
