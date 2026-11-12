use super::{OPS_FILE, ProjectHandle, append_lines, now, read_lines};
use crate::model::*;
use crate::ops::{apply_op, patch_paths, subtree};
use crate::{CoreError, Result};

impl ProjectHandle {
    pub fn apply_ops(&mut self, ops: Vec<Op>, actor: Actor, turn: Option<String>) -> Result<Vec<OpEntry>> {
        if ops.is_empty() {
            return Ok(Vec::new());
        }
        let new_batch = turn.is_none();
        let turn = turn.unwrap_or_else(|| format!("u{}", self.next_batch));
        let mut doc = self.document.clone();
        let mut applied = Vec::new();
        for op in ops {
            let expanded = match op {
                Op::DeleteLayer { id } => subtree(&doc.layers, &id)
                    .into_iter()
                    .rev()
                    .map(|id| Op::DeleteLayer { id })
                    .collect(),
                other => vec![other],
            };
            for op in expanded {
                let op = guard_pins(&doc, op, actor)?;
                let inverse = apply_op(&mut doc, &op)?;
                applied.push((op, inverse));
            }
        }
        let entries = self.record(doc, applied, actor, &turn)?;
        if new_batch {
            self.next_batch += 1;
        }
        let same_turn = !new_batch
            && self
                .undo
                .last()
                .and_then(|s| s.first())
                .and_then(|e| e.turn.as_deref())
                == Some(turn.as_str());
        match (same_turn, self.undo.last_mut()) {
            (true, Some(step)) => step.extend(entries.iter().cloned()),
            _ => self.undo.push(entries.clone()),
        }
        self.redo.clear();
        self.save()?;
        Ok(entries)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> Result<Option<Vec<OpEntry>>> {
        let Some(step) = self.undo.last().cloned() else {
            return Ok(None);
        };
        let entries = self.revert(&step, "undo")?;
        self.undo.pop();
        self.redo.push(entries.clone());
        self.save()?;
        Ok(Some(entries))
    }

    pub fn redo(&mut self) -> Result<Option<Vec<OpEntry>>> {
        let Some(step) = self.redo.last().cloned() else {
            return Ok(None);
        };
        let entries = self.revert(&step, "redo")?;
        self.redo.pop();
        self.undo.push(entries.clone());
        self.save()?;
        Ok(Some(entries))
    }

    pub fn entries_since(&self, seq: u64) -> Result<Vec<OpEntry>> {
        let entries: Vec<OpEntry> = read_lines(&self.dir.join(OPS_FILE))?;
        Ok(entries.into_iter().filter(|e| e.seq > seq).collect())
    }

    pub fn last_seq(&self) -> u64 {
        self.next_seq.saturating_sub(1)
    }

    fn record(
        &mut self,
        doc: Document,
        applied: Vec<(Op, Op)>,
        actor: Actor,
        turn: &str,
    ) -> Result<Vec<OpEntry>> {
        let ts = now();
        let entries: Vec<OpEntry> = applied
            .into_iter()
            .zip(self.next_seq..)
            .map(|((op, inverse), seq)| OpEntry {
                seq,
                actor,
                ts: ts.clone(),
                turn: Some(turn.to_string()),
                op,
                inverse,
            })
            .collect();
        append_lines(&self.dir.join(OPS_FILE), &entries)?;
        self.next_seq += entries.len() as u64;
        self.document = doc;
        self.document.rev += 1;
        Ok(entries)
    }

    fn revert(&mut self, step: &[OpEntry], prefix: &str) -> Result<Vec<OpEntry>> {
        let turn = step.first().and_then(|e| e.turn.as_deref()).unwrap_or_default();
        let base = turn
            .strip_prefix("undo:")
            .or_else(|| turn.strip_prefix("redo:"))
            .unwrap_or(turn);
        let mut doc = self.document.clone();
        let mut applied = Vec::new();
        for entry in step.iter().rev() {
            let inverse = apply_op(&mut doc, &entry.inverse)?;
            applied.push((entry.inverse.clone(), inverse));
        }
        self.record(doc, applied, Actor::User, &format!("{prefix}:{base}"))
    }
}

fn guard_pins(doc: &Document, op: Op, actor: Actor) -> Result<Op> {
    let Op::UpdateLayer { id, mut patch } = op else {
        return Ok(op);
    };
    let Some(layer) = doc.layer(&id) else {
        return Ok(Op::UpdateLayer { id, patch });
    };
    let paths = patch_paths(&patch);
    let Some(map) = patch.as_object_mut().filter(|m| !m.contains_key("pinned")) else {
        return Ok(Op::UpdateLayer { id, patch });
    };
    match actor {
        Actor::Agent => {
            let overlaps = |a: &str, b: &str| {
                a == b || a.starts_with(&format!("{b}.")) || b.starts_with(&format!("{a}."))
            };
            if let Some(path) = paths.iter().find(|p| layer.pinned.iter().any(|q| overlaps(p, q))) {
                return Err(CoreError::invalid(format!(
                    "{} ({id}): {path} was set by the user and is pinned; include \"pinned\" in the patch only if the user asked to change it",
                    layer.name
                )));
            }
        }
        Actor::User => {
            let mut pinned = layer.pinned.clone();
            for path in paths {
                if !pinned.contains(&path) {
                    pinned.push(path);
                }
            }
            if pinned != layer.pinned {
                map.insert("pinned".into(), serde_json::json!(pinned));
            }
        }
    }
    Ok(Op::UpdateLayer { id, patch })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::CoreError;
    use crate::fixtures::{TempRoot, layer, seeded, text};
    use crate::model::*;

    #[test]
    fn same_turn_ops_form_one_undo_step() {
        let root = TempRoot::new();
        let store = root.store();
        let mut h = store.create("t", 100.0, 50.0).expect("create");
        let mk = |id: &str| layer(id, id, None, LayerKind::Group);
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(mk("A")),
                index: None,
            }],
            Actor::Agent,
            Some("turn1".into()),
        )
        .expect("a");
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(mk("B")),
                index: None,
            }],
            Actor::Agent,
            Some("turn1".into()),
        )
        .expect("b");
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(mk("C")),
                index: None,
            }],
            Actor::User,
            None,
        )
        .expect("c");
        assert_eq!(h.document.layers.len(), 3);
        h.undo().expect("undo user");
        assert_eq!(h.document.layers.len(), 2);
        h.undo().expect("undo turn");
        assert_eq!(
            h.document.layers.len(),
            0,
            "both agent ops of turn1 undo together"
        );
        h.redo().expect("redo turn");
        assert_eq!(h.document.layers.len(), 2);
    }

    #[test]
    fn apply_ops_groups_one_undo_step_and_persists() {
        let root = TempRoot::new();
        let store = root.store();
        let mut h = seeded(&store);
        assert_eq!(h.last_seq(), 6);
        let before = h.document.clone();

        let entries = h
            .apply_ops(
                vec![
                    Op::UpdateLayer {
                        id: "L3".into(),
                        patch: json!({"x_mm": 130}),
                    },
                    Op::DeleteLayer { id: "L2".into() },
                ],
                Actor::User,
                None,
            )
            .expect("apply");
        assert_eq!(entries.len(), 5);
        assert!(entries.iter().all(|e| e.turn.as_deref() == Some("u1")));
        assert_eq!(
            entries.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [7, 8, 9, 10, 11]
        );
        assert_eq!(h.document.rev, before.rev + 1);
        assert_eq!(h.document.layers.len(), 2);

        let reopened = store.open(&h.project.id).expect("open");
        assert_eq!(reopened.document, h.document);
        assert!(reopened.can_undo());
        drop(h);

        let mut h = reopened;
        let undone = h.undo().expect("undo").expect("step");
        assert_eq!(undone.len(), 5);
        assert!(undone.iter().all(|e| e.turn.as_deref() == Some("undo:u1")));
        assert_eq!(h.document.layers, before.layers);
        assert!(h.can_redo());

        let mut h = store.open(&h.project.id).expect("reopen");
        let redone = h.redo().expect("redo").expect("step");
        assert!(redone.iter().all(|e| e.turn.as_deref() == Some("redo:u1")));
        assert_eq!(h.document.layers.len(), 2);
        h.undo().expect("undo");
        h.undo().expect("undo seed");
        assert!(h.document.layers.is_empty());
        assert_eq!(h.undo().expect("nothing"), None);

        let all = h.entries_since(0).expect("entries");
        assert_eq!(all.len() as u64, h.last_seq());
        assert_eq!(
            h.entries_since(10).expect("since").len() as u64,
            h.last_seq() - 10
        );
    }

    #[test]
    fn new_ops_clear_redo_and_failed_batch_changes_nothing() {
        let root = TempRoot::new();
        let mut h = seeded(&root.store());
        h.undo().expect("undo");
        let snapshot = h.document.clone();
        let seq = h.last_seq();
        let r = h.apply_ops(
            vec![
                Op::AddLayer {
                    layer: Box::new(layer("L1", "a", None, text("a"))),
                    index: None,
                },
                Op::DeleteLayer { id: "missing".into() },
            ],
            Actor::User,
            None,
        );
        assert!(matches!(r, Err(CoreError::LayerNotFound(_))));
        assert_eq!(h.document, snapshot);
        assert_eq!(h.last_seq(), seq);
        assert!(h.can_redo());

        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(layer("L1", "a", None, text("a"))),
                index: None,
            }],
            Actor::User,
            None,
        )
        .expect("apply");
        assert!(!h.can_redo());
    }

    #[test]
    fn user_edits_pin_and_agent_respects_pins() {
        let root = TempRoot::new();
        let mut h = seeded(&root.store());
        let entries = h
            .apply_ops(
                vec![Op::UpdateLayer {
                    id: "L3".into(),
                    patch: json!({"kind": {"text": "새 문구"}, "x_mm": 120}),
                }],
                Actor::User,
                None,
            )
            .expect("user edit");
        let mut pinned = h.document.layer("L3").expect("L3").pinned.clone();
        pinned.sort();
        assert_eq!(pinned, ["kind.text", "x_mm"]);

        for patch in [
            json!({"x_mm": 1}),
            json!({"kind": {"type": "text", "text": "x"}}),
            json!({"kind": null}),
        ] {
            let r = h.apply_ops(
                vec![Op::UpdateLayer {
                    id: "L3".into(),
                    patch,
                }],
                Actor::Agent,
                Some("t2".into()),
            );
            assert!(matches!(r, Err(CoreError::InvalidOp(_))));
        }
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L3".into(),
                patch: json!({"y_mm": 1}),
            }],
            Actor::Agent,
            Some("t2".into()),
        )
        .expect("unpinned path");
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L3".into(),
                patch: json!({"x_mm": 1, "pinned": ["kind.text"]}),
            }],
            Actor::Agent,
            Some("t3".into()),
        )
        .expect("explicit unpin");
        assert_eq!(h.document.layer("L3").expect("L3").pinned, ["kind.text"]);

        h.undo().expect("undo t3");
        h.undo().expect("undo t2");
        h.undo().expect("undo user edit");
        assert!(h.document.layer("L3").expect("L3").pinned.is_empty());
        assert_eq!(entries.len(), 1);
    }
}
