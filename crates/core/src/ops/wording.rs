use serde_json::Value;

use super::patch_paths;
use crate::model::*;

pub fn describe(op: &Op, before: &Document) -> String {
    let name = |id: &str| {
        before
            .layer(id)
            .map_or_else(|| id.to_string(), |l| l.name.clone())
    };
    match op {
        Op::AddLayer { layer, .. } => format!("레이어 추가 · {}", layer.name),
        Op::DeleteLayer { id } => format!("레이어 삭제 · {}", name(id)),
        Op::MoveLayer {
            id, parent: Some(p), ..
        } => format!("레이어 이동 · {} → {}", name(id), name(p)),
        Op::MoveLayer { id, parent: None, .. } => format!("레이어 이동 · {}", name(id)),
        Op::UpdateLayer { id, patch } => {
            let old = before
                .layer(id)
                .and_then(|l| serde_json::to_value(l).ok())
                .unwrap_or(Value::Null);
            format!("{} · {}", name(id), change_summary(&old, patch))
        }
        Op::SetArtboard { patch } => {
            let old = serde_json::to_value(&before.artboard).unwrap_or(Value::Null);
            format!("아트보드 · {}", change_summary(&old, patch))
        }
    }
}

/// Uses the properties panel's own words, so the history and the agent's work log never show `w_mm` or `kind.font_family`.
pub fn field_label(path: &str) -> &'static str {
    match path {
        "x_mm" => "X",
        "y_mm" => "Y",
        "w_mm" | "width_mm" => "가로",
        "h_mm" | "height_mm" => "세로",
        "rotation_deg" => "회전",
        "opacity" => "불투명",
        "name" => "이름",
        "visible" => "보이기",
        "locked" => "잠금",
        "parent" => "묶음",
        "pinned" => "고정",
        "bleed_mm" => "재단",
        "kind.type" => "종류",
        "kind.text" => "문구",
        "kind.font_family" => "글꼴",
        "kind.font_weight" => "굵기",
        "kind.font_size_mm" => "글자 크기",
        "kind.letter_spacing_mm" => "자간",
        "kind.line_height" => "행간",
        "kind.align" => "정렬",
        "kind.scale_x" => "장평",
        "kind.skew_deg" => "기울임",
        "kind.arc_bend" => "아치",
        "kind.vertical" => "세로쓰기",
        "kind.rx_mm" => "둥근 모서리",
        "kind.d" | "kind.fill_rule" => "모양",
        "kind.fit" => "맞춤",
        "kind.crop_px" => "자르기",
        p if p.starts_with("fills") => "채우기",
        p if p.starts_with("strokes") => "외곽선",
        p if p.starts_with("effects") => "효과",
        p if p.starts_with("background") => "배경색",
        p if p.starts_with("kind.adjust") => "보정",
        p if p.starts_with("kind.asset") || p.starts_with("kind.natural") => "그림",
        p if p.starts_with("strategy") || p.starts_with("source") => "만드는 방식",
        p if p.starts_with("kind.") => "도형 모양",
        _ => "속성",
    }
}

fn change_summary(old: &Value, patch: &Value) -> String {
    let mut paths = patch_paths(patch);
    if paths.len() > 1 {
        paths.retain(|p| p != "pinned");
    }
    let mut parts: Vec<String> = paths
        .iter()
        .take(3)
        .map(|path| {
            let pointer = format!("/{}", path.replace('.', "/"));
            format!(
                "{} {} → {}",
                field_label(path),
                value_text(old.pointer(&pointer)),
                value_text(patch.pointer(&pointer))
            )
        })
        .collect();
    if paths.len() > 3 {
        parts.push(format!("외 {}개", paths.len() - 3));
    }
    parts.join(", ")
}

fn value_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "없음".into(),
        Some(Value::Number(n)) => {
            let v = n.as_f64().unwrap_or(0.0);
            let s = format!("{v:.2}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        Some(Value::String(s)) if s.chars().count() > 20 => {
            format!("{}…", s.chars().take(20).collect::<String>())
        }
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => format!("{}개", a.len()),
        Some(Value::Object(_)) => "{…}".into(),
        Some(Value::Bool(b)) => b.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::fixtures::{layer, sample};

    #[test]
    fn describe_summarizes_in_korean() {
        let doc = sample();
        assert_eq!(
            describe(
                &Op::UpdateLayer {
                    id: "L3".into(),
                    patch: json!({"x_mm": 130})
                },
                &doc
            ),
            "청전동 주민역량강화 · X 100 → 130"
        );
        assert_eq!(
            describe(
                &Op::AddLayer {
                    layer: Box::new(layer("L9", "정보 띠", None, LayerKind::Group)),
                    index: None
                },
                &doc
            ),
            "레이어 추가 · 정보 띠"
        );
        assert_eq!(
            describe(&Op::DeleteLayer { id: "L4".into() }, &doc),
            "레이어 삭제 · 문인화"
        );
        assert_eq!(
            describe(
                &Op::SetArtboard {
                    patch: json!({"width_mm": 1200.5})
                },
                &doc
            ),
            "아트보드 · 가로 900 → 1200.5"
        );
    }
}
