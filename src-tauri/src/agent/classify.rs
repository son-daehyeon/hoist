use std::time::Duration;

use hoist_core::model::{Layer, LayerKind};
use hoist_core::store::ProjectHandle;
use serde_json::{Value, json};

use crate::openai::{OpenAI, Usage};

const TIMEOUT: Duration = Duration::from_secs(4);
const SIMPLE_MIN: f64 = 0.6;
const RELAYOUT_MIN: f64 = 0.6;
const REBUILD_MIN: f64 = 0.8;
const CROWDED_MIN: f64 = 0.7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnClass {
    Simple,
    Normal,
    Relayout,
    Rebuild,
}

impl TurnClass {
    /// Used when the classifier is skipped, fails, times out or is unsure.
    pub fn from_keywords(text: &str) -> TurnClass {
        if ["처음부터", "다시 만들어"].iter().any(|k| text.contains(k)) {
            TurnClass::Rebuild
        } else {
            TurnClass::Normal
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TurnClass::Simple => "간단한 수정",
            TurnClass::Normal => "일반 수정",
            TurnClass::Relayout => "재배치",
            TurnClass::Rebuild => "처음부터",
        }
    }
}

/// The classifier's choice only counts when it is sure enough; a rebuild throws the layers away, so it needs more.
fn decided(choice: &str, probability: f64) -> Option<TurnClass> {
    match choice {
        "simple" if probability >= SIMPLE_MIN => Some(TurnClass::Simple),
        "relayout" if probability >= RELAYOUT_MIN => Some(TurnClass::Relayout),
        "rebuild" if probability >= REBUILD_MIN => Some(TurnClass::Rebuild),
        "normal" => Some(TurnClass::Normal),
        _ => None,
    }
}

pub struct Verdict {
    pub class: Option<TurnClass>,
    pub choice: String,
    pub probability: f64,
    pub usage: Usage,
}

fn question() -> Value {
    json!({
        "type": "choice",
        "name": "kind",
        "instructions": "A user of a banner-editing app sent this request about a layered banner document (the request is Korean; the selected layers, if any, are listed). Which kind of work is it?",
        "choices": [
            {"value": "simple", "description": "Change a few properties of the named or selected layers only: wording, a phone number or date, a color, a font size, moving or resizing one element. No judgement about other layers or the original picture is needed."},
            {"value": "normal", "description": "An edit that needs judgement across several layers or the original picture: fixing or redrawing a picture, matching a font, adjusting a group of elements, or anything unclear."},
            {"value": "relayout", "description": "Rearrange the whole banner's layout: margins, spacing, re-flowing rows or columns, making it less cramped."},
            {"value": "rebuild", "description": "Start over: rebuild the whole banner from scratch or from the original again."}
        ]
    })
}

fn layer_line(l: &Layer) -> String {
    let what = match &l.kind {
        LayerKind::Text(t) => format!("text \"{}\"", t.text.chars().take(40).collect::<String>()),
        LayerKind::Image(_) => "picture".to_string(),
        LayerKind::Shape(_) => "shape".to_string(),
        LayerKind::Path(_) => "path".to_string(),
        LayerKind::Group => "group".to_string(),
    };
    format!("- {} ({what}, {:.0}×{:.0} mm)", l.name, l.w_mm, l.h_mm)
}

pub fn input(h: &ProjectHandle, text: &str, layer_ids: &[String]) -> String {
    let mut s = format!(
        "Request: {text}\nDocument: {} layers, {} × {} mm",
        h.document.layers.len(),
        h.document.artboard.width_mm,
        h.document.artboard.height_mm
    );
    let selected: Vec<String> = layer_ids
        .iter()
        .filter_map(|id| h.document.layer(id))
        .map(layer_line)
        .collect();
    if !selected.is_empty() {
        s.push_str("\nSelected layers:\n");
        s.push_str(&selected.join("\n"));
    }
    s
}

pub async fn classify(openai: &OpenAI, model: &str, input: &str) -> anyhow::Result<Verdict> {
    let out = openai.decide(model, json!(input), &[question()], TIMEOUT).await?;
    let (choice, probability) = out
        .choice("kind")
        .ok_or_else(|| anyhow::anyhow!("decisions 응답에 kind 답이 없어요"))?;
    Ok(Verdict {
        class: decided(&choice, probability),
        choice,
        probability,
        usage: out.usage,
    })
}

pub struct Review {
    pub crowded: f64,
    pub usage: Usage,
}

impl Review {
    pub fn failed(&self) -> bool {
        self.crowded >= CROWDED_MIN
    }
}

pub async fn review(openai: &OpenAI, model: &str, work_png: &[u8]) -> anyhow::Result<Review> {
    let input = json!([{
        "role": "user",
        "content": [
            {"type": "input_text", "text": "The current state of a wide outdoor banner being laid out in a banner-editing app."},
            {"type": "input_image", "image_url": super::img::data_url(work_png)}
        ]
    }]);
    let question = json!({
        "type": "predicate",
        "name": "crowded",
        "instructions": "Is the banner packed to its edges — text or boxes touching the rim, rows with no air between them, the title crowded by other elements — so that it reads cramped rather than calm?"
    });
    let out = openai.decide(model, input, &[question], TIMEOUT).await?;
    let crowded = out
        .probability("crowded")
        .ok_or_else(|| anyhow::anyhow!("decisions 응답에 crowded 답이 없어요"))?;
    Ok(Review {
        crowded,
        usage: out.usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsure_classifier_leaves_the_choice_to_the_keywords() {
        assert_eq!(decided("simple", 0.9), Some(TurnClass::Simple));
        assert_eq!(decided("simple", 0.5), None);
        assert_eq!(decided("normal", 0.3), Some(TurnClass::Normal));
        assert_eq!(decided("relayout", 0.7), Some(TurnClass::Relayout));
        assert_eq!(decided("rebuild", 0.75), None, "a rebuild needs to be sure");
        assert_eq!(decided("rebuild", 0.9), Some(TurnClass::Rebuild));
        assert_eq!(decided("something_else", 1.0), None);
        assert_eq!(TurnClass::from_keywords("처음부터 다시 해줘"), TurnClass::Rebuild);
        assert_eq!(TurnClass::from_keywords("제목을 키워줘"), TurnClass::Normal);
    }

    #[test]
    fn the_classifier_input_names_the_selected_layers() {
        let root = std::env::temp_dir().join(format!("hoist-classify-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root)
            .create("classify", 6000.0, 900.0)
            .expect("project");
        h.document.layers = vec![Layer {
            id: "L1".into(),
            name: "제목".into(),
            parent: None,
            kind: LayerKind::Text(hoist_core::model::TextProps {
                text: "주민역량강화".into(),
                ..Default::default()
            }),
            x_mm: 0.0,
            y_mm: 0.0,
            w_mm: 1578.0,
            h_mm: 306.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![],
            strokes: vec![],
            effects: vec![],
            strategy: hoist_core::model::Strategy::NativeVector,
            strategy_reason: String::new(),
            pinned: vec![],
            source: None,
        }];
        let s = input(&h, "주민사기강화로 바꿔", &["L1".into(), "missing".into()]);
        assert!(
            s.starts_with("Request: 주민사기강화로 바꿔\nDocument: 1 layers"),
            "{s}"
        );
        assert!(s.contains("- 제목 (text \"주민역량강화\", 1578×306 mm)"), "{s}");
        assert!(!input(&h, "x", &[]).contains("Selected layers"));
    }
}
