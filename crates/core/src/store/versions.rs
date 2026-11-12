use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::{
    ProjectHandle, VERSION_INDEX_FILE, VERSIONS_DIR, now, read_json, read_json_opt, write_atomic, write_json,
};
use crate::Result;
use crate::model::*;
use crate::ops::{apply_op, diff, sibling_index};

impl ProjectHandle {
    pub fn snapshot(
        &mut self,
        actor: Actor,
        summary: &str,
        turn: Option<String>,
        thumbnail_png: Option<&[u8]>,
    ) -> Result<Version> {
        let seq = self.last_seq();
        let dir = self.dir.join(VERSIONS_DIR);
        fs::create_dir_all(&dir)?;
        write_json(&dir.join(format!("{seq}.json")), &self.document)?;
        if let Some(png) = thumbnail_png {
            write_atomic(&self.version_thumbnail_path(seq), png)?;
        }
        let version = Version {
            seq,
            ts: now(),
            actor,
            summary: summary.to_string(),
            turn,
        };
        let mut index: Vec<Version> = read_json_opt(&dir.join(VERSION_INDEX_FILE))?.unwrap_or_default();
        index.retain(|v| v.seq != seq);
        index.push(version.clone());
        write_json(&dir.join(VERSION_INDEX_FILE), &index)?;
        Ok(version)
    }

    pub fn versions(&self) -> Result<Vec<Version>> {
        let mut index: Vec<Version> =
            read_json_opt(&self.dir.join(VERSIONS_DIR).join(VERSION_INDEX_FILE))?.unwrap_or_default();
        index.sort_by_key(|v| std::cmp::Reverse(v.seq));
        Ok(index)
    }

    pub fn version_document(&self, seq: u64) -> Result<Document> {
        read_json(&self.dir.join(VERSIONS_DIR).join(format!("{seq}.json")))
    }

    pub fn version_thumbnail_path(&self, seq: u64) -> PathBuf {
        self.dir.join(VERSIONS_DIR).join(format!("{seq}.png"))
    }

    pub fn restore_version(&mut self, seq: u64) -> Result<Vec<OpEntry>> {
        let target = self.version_document(seq)?;
        let ops = restore_ops(&self.document, &target)?;
        self.apply_ops(ops, Actor::User, Some(format!("restore:{seq}")))
    }
}

fn restore_ops(current: &Document, target: &Document) -> Result<Vec<Op>> {
    let mut wanted = Vec::new();
    preorder(target, None, &mut wanted);
    let mut doc = current.clone();
    let mut ops = Vec::new();
    detach_from_removed_parents(&mut doc, &mut ops, &wanted)?;
    delete_missing(&mut doc, &mut ops, &wanted)?;
    let ungroup = patch_existing(&mut doc, &mut ops, &wanted)?;
    add_missing(&mut doc, &mut ops, &wanted, target)?;
    reorder(&mut doc, &mut ops, &wanted, target)?;
    for op in ungroup {
        push(&mut doc, &mut ops, op)?;
    }
    set_artboard(&mut doc, &mut ops, target)?;
    Ok(ops)
}

fn push(doc: &mut Document, ops: &mut Vec<Op>, op: Op) -> Result<()> {
    apply_op(doc, &op)?;
    ops.push(op);
    Ok(())
}

fn in_target(wanted: &[&Layer], id: &str) -> bool {
    wanted.iter().any(|l| l.id == id)
}

fn detach_from_removed_parents(doc: &mut Document, ops: &mut Vec<Op>, wanted: &[&Layer]) -> Result<()> {
    let detach: Vec<LayerId> = doc
        .layers
        .iter()
        .filter(|l| in_target(wanted, &l.id) && l.parent.as_deref().is_some_and(|p| !in_target(wanted, p)))
        .map(|l| l.id.clone())
        .collect();
    for id in detach {
        let index = doc.layers.len();
        push(
            doc,
            ops,
            Op::MoveLayer {
                id,
                parent: None,
                index,
            },
        )?;
    }
    Ok(())
}

fn delete_missing(doc: &mut Document, ops: &mut Vec<Op>, wanted: &[&Layer]) -> Result<()> {
    let doomed: Vec<LayerId> = doc
        .layers
        .iter()
        .filter(|l| !in_target(wanted, &l.id) && l.parent.as_deref().is_none_or(|p| in_target(wanted, p)))
        .map(|l| l.id.clone())
        .collect();
    for id in doomed {
        push(doc, ops, Op::DeleteLayer { id })?;
    }
    Ok(())
}

fn patch_existing(doc: &mut Document, ops: &mut Vec<Op>, wanted: &[&Layer]) -> Result<Vec<Op>> {
    let mut ungroup = Vec::new();
    for t in wanted {
        let Some(cur) = doc.layer(&t.id) else { continue };
        let Some(patch) = layer_patch(cur, t)? else {
            continue;
        };
        let op = Op::UpdateLayer {
            id: t.id.clone(),
            patch,
        };
        if matches!(cur.kind, LayerKind::Group) && !matches!(t.kind, LayerKind::Group) {
            ungroup.push(op);
        } else {
            push(doc, ops, op)?;
        }
    }
    Ok(ungroup)
}

fn add_missing(doc: &mut Document, ops: &mut Vec<Op>, wanted: &[&Layer], target: &Document) -> Result<()> {
    for t in wanted {
        if doc.layer(&t.id).is_none() {
            let index = Some(sibling_index(target, t));
            push(
                doc,
                ops,
                Op::AddLayer {
                    layer: Box::new((*t).clone()),
                    index,
                },
            )?;
        }
    }
    Ok(())
}

fn reorder(doc: &mut Document, ops: &mut Vec<Op>, wanted: &[&Layer], target: &Document) -> Result<()> {
    for t in wanted {
        let index = sibling_index(target, t);
        let Some(cur) = doc.layer(&t.id) else { continue };
        if cur.parent != t.parent || sibling_index(doc, cur) != index {
            push(
                doc,
                ops,
                Op::MoveLayer {
                    id: t.id.clone(),
                    parent: t.parent.clone(),
                    index,
                },
            )?;
        }
    }
    Ok(())
}

fn set_artboard(doc: &mut Document, ops: &mut Vec<Op>, target: &Document) -> Result<()> {
    let artboard = diff(
        &serde_json::to_value(&doc.artboard)?,
        &serde_json::to_value(&target.artboard)?,
    );
    if artboard.as_object().is_some_and(|m| !m.is_empty()) {
        push(doc, ops, Op::SetArtboard { patch: artboard })?;
    }
    Ok(())
}

fn layer_patch(cur: &Layer, target: &Layer) -> Result<Option<Value>> {
    let mut patch = diff(&serde_json::to_value(cur)?, &serde_json::to_value(target)?);
    let Some(map) = patch.as_object_mut() else {
        return Ok(None);
    };
    map.remove("parent");
    if map.is_empty() {
        return Ok(None);
    }
    map.insert("pinned".into(), serde_json::to_value(&target.pinned)?);
    Ok(Some(patch))
}

fn preorder<'a>(doc: &'a Document, parent: Option<&str>, out: &mut Vec<&'a Layer>) {
    for layer in doc.children(parent) {
        out.push(layer);
        preorder(doc, Some(&layer.id), out);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::fixtures::{TempRoot, layer, seeded};
    use crate::model::*;

    #[test]
    fn snapshot_and_restore_keep_history() {
        let root = TempRoot::new();
        let mut h = seeded(&root.store());
        let v = h
            .snapshot(Actor::Agent, "초안", Some("t1".into()), Some(&[9, 9]))
            .expect("snapshot");
        assert_eq!(v.seq, 6);
        assert!(h.version_thumbnail_path(6).is_file());
        let saved = h.document.clone();

        h.apply_ops(
            vec![
                Op::MoveLayer { id: "L3".into(), parent: None, index: 0 },
                Op::DeleteLayer { id: "L2".into() },
                Op::AddLayer { layer: Box::new(layer("L7", "새 그룹", None, LayerKind::Group)), index: Some(0) },
                Op::MoveLayer { id: "L6".into(), parent: Some("L7".into()), index: 0 },
                Op::UpdateLayer { id: "L1".into(), patch: json!({"kind": {"type": "text", "text": "x", "font_family": "A", "font_size_mm": 5}}) },
                Op::SetArtboard { patch: json!({"width_mm": 1000, "bleed_mm": 3}) },
            ],
            Actor::User,
            None,
        )
        .expect("mutate");
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L7".into(),
                patch: json!({"kind": {"type": "shape", "shape": "ellipse"}}),
            }],
            Actor::User,
            None,
        )
        .expect_err("group with children");
        h.apply_ops(
            vec![Op::MoveLayer {
                id: "L6".into(),
                parent: None,
                index: 0,
            }],
            Actor::User,
            None,
        )
        .expect("move out");
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L7".into(),
                patch: json!({"kind": {"type": "shape", "shape": "ellipse"}}),
            }],
            Actor::User,
            None,
        )
        .expect("degroup");
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(layer("L8", "그룹", None, LayerKind::Group)),
                index: None,
            }],
            Actor::User,
            None,
        )
        .expect("group");
        h.apply_ops(
            vec![Op::MoveLayer {
                id: "L1".into(),
                parent: Some("L8".into()),
                index: 0,
            }],
            Actor::User,
            None,
        )
        .expect("nest");
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L1".into(),
                patch: json!({"kind": {"type": "group"}}),
            }],
            Actor::User,
            None,
        )
        .expect("L1 becomes group");
        h.apply_ops(
            vec![Op::MoveLayer {
                id: "L1".into(),
                parent: None,
                index: 0,
            }],
            Actor::User,
            None,
        )
        .expect("unnest");
        h.apply_ops(
            vec![Op::MoveLayer {
                id: "L3".into(),
                parent: Some("L1".into()),
                index: 0,
            }],
            Actor::User,
            None,
        )
        .expect("L3 under L1");
        let mutated = h.document.clone();

        let entries = h.restore_version(6).expect("restore");
        assert!(entries.iter().all(|e| e.turn.as_deref() == Some("restore:6")));
        assert_eq!(h.document.layers, saved.layers);
        assert_eq!(h.document.artboard, saved.artboard);

        h.undo().expect("undo restore");
        assert_eq!(h.document.layers, mutated.layers);
        assert_eq!(h.document.artboard, mutated.artboard);

        let versions = h.versions().expect("versions");
        assert_eq!(versions.len(), 1);
        h.snapshot(Actor::User, "수정", None, None).expect("snapshot 2");
        assert_eq!(h.versions().expect("versions")[0].summary, "수정");
        assert_eq!(h.version_document(h.last_seq()).expect("doc"), h.document);
    }
}
