use hoist_core::model::{Actor, Document, Op, OpEntry};
use hoist_core::ops::describe;
use hoist_core::store::ProjectHandle;
use serde_json::Value;

pub fn describe_entry(e: &OpEntry, doc: &Document) -> String {
    let name = |id: &str| {
        doc.layer(id)
            .map(|l| l.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    match (&e.op, &e.inverse) {
        (Op::UpdateLayer { id, patch }, Op::UpdateLayer { patch: old, .. }) => {
            let paths: Vec<String> = hoist_core::ops::patch_paths(patch)
                .into_iter()
                .filter(|p| p != "pinned")
                .collect();
            let show = |v: Option<&Value>| match v {
                None | Some(Value::Null) => "없음".to_string(),
                Some(Value::Number(n)) => {
                    let s = format!("{:.2}", n.as_f64().unwrap_or(0.0));
                    s.trim_end_matches('0').trim_end_matches('.').to_string()
                }
                Some(Value::String(s)) => s.chars().take(20).collect(),
                Some(Value::Array(a)) => format!("{}개", a.len()),
                Some(Value::Object(_)) => "{…}".into(),
                Some(Value::Bool(b)) => b.to_string(),
            };
            let mut parts: Vec<String> = paths
                .iter()
                .take(3)
                .map(|p| {
                    let ptr = format!("/{}", p.replace('.', "/"));
                    format!(
                        "{} {} → {}",
                        hoist_core::ops::field_label(p),
                        show(old.pointer(&ptr)),
                        show(patch.pointer(&ptr))
                    )
                })
                .collect();
            if paths.len() > 3 {
                parts.push(format!("외 {}개", paths.len() - 3));
            }
            format!("{} · {}", name(id), parts.join(", "))
        }
        (Op::DeleteLayer { .. }, Op::AddLayer { layer, .. }) => format!("레이어 삭제 · {}", layer.name),
        (op, _) => describe(op, doc),
    }
}

pub(super) fn edit_summary(h: &ProjectHandle, since_seq: u64) -> Vec<String> {
    let entries = h.entries_since(since_seq).unwrap_or_default();
    let mut lines: Vec<String> = Vec::new();
    for e in entries.iter().filter(|e| {
        e.actor == Actor::User
            && !["undo:", "redo:", "restore:"]
                .iter()
                .any(|p| e.turn.as_deref().unwrap_or("").starts_with(p))
    }) {
        let line = describe_entry(e, &h.document);
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines.dedup();
    if lines.len() > 12 {
        let n = lines.len();
        lines.truncate(11);
        lines.push(format!("… 외 {}건", n - 11));
    }
    lines
}
