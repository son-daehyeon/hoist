use hoist_core::image as cimg;
use hoist_core::model::{Document, Layer, LayerKind};
use hoist_core::render::layer_bounds_mm;
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;
use serde_json::{Value, json};

use super::classify::TurnClass;
use super::context::input_image;
use super::events::{Pane, SendContext};
use super::img;
use super::prompt::{self, OriginalFacts, TurnKind};

const SELECTED_LAYER_PX: u32 = 800;
const CAPTURE_PX: u32 = 1000;
const ORIGINAL_PX: u32 = 1536;

pub(super) fn message_text(
    h: &ProjectHandle,
    text: &str,
    sent_context: &SendContext,
    summary: &[String],
    kind: TurnKind,
    class: TurnClass,
) -> String {
    let mut msg = text.to_string();
    if !summary.is_empty() {
        msg.push_str("\n\n<pinned_by_user>\nThe user edited these by hand; they stay fixed unless this message asks otherwise.\n");
        for l in summary {
            msg.push_str("- ");
            msg.push_str(l);
            msg.push('\n');
        }
        msg.push_str("</pinned_by_user>\n");
    }
    if !sent_context.layer_ids.is_empty() {
        msg.push_str("\n<selected_layers>\n");
        for id in &sent_context.layer_ids {
            if let Some(l) = h.document.layer(id) {
                msg.push_str(&selected_layer_line(l));
            }
        }
        msg.push_str("</selected_layers>\n");
    }
    if class == TurnClass::Simple {
        if sent_context.layer_ids.is_empty() {
            msg.push_str(&text_layer_list(&h.document));
        }
        msg.push('\n');
        msg.push_str(prompt::SIMPLE_TURN_NOTE);
        msg.push('\n');
    }
    for c in &sent_context.captures {
        msg.push_str(&format!(
            "\n<capture pane=\"{}\" x_mm=\"{:.0}\" y_mm=\"{:.0}\" w_mm=\"{:.0}\" h_mm=\"{:.0}\"/> (image attached below)\n",
            if c.pane == Pane::Original {
                "original"
            } else {
                "work"
            },
            c.x_mm,
            c.y_mm,
            c.w_mm,
            c.h_mm
        ));
    }
    if kind == TurnKind::ReplaceOriginal {
        msg.push('\n');
        msg.push_str(prompt::REPLACE_PROMPT);
    }
    msg
}

pub(super) fn attachments(
    h: &ProjectHandle,
    fonts: &Fonts,
    facts: Option<&OriginalFacts>,
    sent_context: &SendContext,
    kind: TurnKind,
) -> anyhow::Result<Vec<Value>> {
    let mut parts: Vec<Value> = Vec::new();
    for id in &sent_context.layer_ids {
        if let Some(l) = h.document.layer(id) {
            let b = layer_bounds_mm(&h.document, l);
            let rect = img::clamp_rect_mm(h, [b[0] - 5.0, b[1] - 5.0, b[2] + 10.0, b[3] + 10.0]);
            if let Ok(im) = img::work_region(&h.document, &h.assets_dir(), fonts, rect, SELECTED_LAYER_PX) {
                parts.push(input_image(&cimg::to_png_bytes(&im)?));
            }
        }
    }
    for c in &sent_context.captures {
        let rect = img::clamp_rect_mm(h, [c.x_mm, c.y_mm, c.w_mm, c.h_mm]);
        let im = if c.pane == Pane::Original {
            match facts {
                Some(f) => {
                    let orig = img::original_image(h, f.version)?;
                    let px = |mm: f64, off: f64| ((mm - off) / f.mm_per_px).max(0.0) as u32;
                    cimg::crop(
                        &orig,
                        [
                            px(rect[0], f.offset_x_mm),
                            px(rect[1], f.offset_y_mm),
                            (rect[2] / f.mm_per_px).max(1.0) as u32,
                            (rect[3] / f.mm_per_px).max(1.0) as u32,
                        ],
                    )
                }
                None => continue,
            }
        } else {
            img::work_region(&h.document, &h.assets_dir(), fonts, rect, CAPTURE_PX)?
        };
        parts.push(input_image(&cimg::to_png_bytes(&cimg::resize_max(
            &im, CAPTURE_PX,
        ))?));
    }
    if (kind == TurnKind::Reconstruct || kind == TurnKind::ReplaceOriginal)
        && let Some(f) = facts
    {
        let orig = img::original_image(h, f.version)?;
        parts.push(json!({"type": "input_text", "text": format!("<original version=\"{}\"/> full image attached below", f.version)}));
        parts.push(input_image(&cimg::to_png_bytes(&cimg::resize_max(
            &orig,
            ORIGINAL_PX,
        ))?));
    }
    Ok(parts)
}

const SELECTED_LAYER_MAX_CHARS: usize = 4000;

/// The full record as `get_layer` returns it, so an edit needs no read round first; a long traced path keeps
/// to the one-line summary.
fn selected_layer_line(l: &Layer) -> String {
    let record = serde_json::to_string(l).unwrap_or_default();
    if record.chars().count() <= SELECTED_LAYER_MAX_CHARS {
        format!("- {record}\n")
    } else {
        format!(
            "- {} ({}) · {:.0},{:.0} {:.0}×{:.0} mm (long record: get_layer for the rest)\n",
            l.id, l.name, l.x_mm, l.y_mm, l.w_mm, l.h_mm
        )
    }
}

/// With nothing selected a simple edit names its target by wording, so every text layer's record goes along:
/// the same records a selection would carry, because the edit needs them and a read would cost a round.
fn text_layer_list(doc: &Document) -> String {
    let lines: String = doc
        .layers
        .iter()
        .filter(|l| matches!(l.kind, LayerKind::Text(_)))
        .map(selected_layer_line)
        .collect();
    format!("\n<text_layers>\n{lines}</text_layers>\n")
}

#[cfg(test)]
mod tests {
    use super::super::layer_args;
    use super::*;

    #[test]
    fn a_simple_turn_without_a_selection_lists_the_wording() {
        let mut doc = Document::new(6000.0, 900.0);
        let raw = json!({"name": "제목", "kind": {"type": "text", "text": "주민\n역량", "font_family": "Pretendard", "font_size_mm": 100}, "x_mm": 0, "y_mm": 0, "w_mm": 900, "h_mm": 300});
        let layer = layer_args::parse_layer(&raw, (6000.0, 900.0)).expect("text layer");
        doc.layers.push(layer);
        let list = text_layer_list(&doc);
        assert!(list.contains("\"text\":\"주민\\n역량\""), "{list}");
        assert!(list.contains("\"font_family\":\"Pretendard\""), "{list}");
    }

    #[test]
    fn a_simple_turn_carries_the_layer_records_and_the_note() {
        let root = std::env::temp_dir().join(format!("hoist-simple-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root)
            .create("simple", 6000.0, 900.0)
            .expect("project");
        let layer = |id: &str, kind: hoist_core::model::LayerKind| hoist_core::model::Layer {
            id: id.into(),
            name: id.into(),
            parent: None,
            kind,
            x_mm: 100.0,
            y_mm: 200.0,
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
        };
        h.document.layers = vec![
            layer(
                "제목",
                hoist_core::model::LayerKind::Text(hoist_core::model::TextProps {
                    text: "주민역량강화".into(),
                    font_family: "Pretendard".into(),
                    font_size_mm: 300.0,
                    ..Default::default()
                }),
            ),
            layer(
                "붓",
                hoist_core::model::LayerKind::Path(hoist_core::model::PathProps {
                    d: "M 0 0 L 1 1 ".repeat(SELECTED_LAYER_MAX_CHARS / 10),
                    fill_rule: Default::default(),
                }),
            ),
        ];
        let fonts = Fonts::new(None);
        let ctx = SendContext {
            layer_ids: vec!["제목".into(), "붓".into()],
            captures: vec![],
        };
        attachments(&h, &fonts, None, &ctx, TurnKind::Normal).expect("parts");
        let msg = message_text(
            &h,
            "주민사기강화로 바꿔",
            &ctx,
            &[],
            TurnKind::Normal,
            TurnClass::Simple,
        );
        assert!(
            msg.contains("\"text\":\"주민역량강화\"") && msg.contains("\"font_size_mm\":300.0"),
            "the text layer's record travels with the message: {msg}"
        );
        assert!(
            msg.contains("- 붓 (붓) · 100,200 1578×306 mm (long record: get_layer for the rest)"),
            "a long path keeps to one line: {msg}"
        );
        assert!(msg.contains("<turn class=\"simple\">"), "{msg}");
        let msg = message_text(
            &h,
            "주민사기강화로 바꿔",
            &ctx,
            &[],
            TurnKind::Normal,
            TurnClass::Normal,
        );
        assert!(
            msg.contains("\"text\":\"주민역량강화\"") && !msg.contains("<turn class="),
            "{msg}"
        );
    }
}
