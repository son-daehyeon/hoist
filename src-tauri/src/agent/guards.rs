use anyhow::anyhow;
use hoist_core::image as cimg;
use hoist_core::model::FitMode;
use hoist_core::store::ProjectHandle;
use serde_json::{Value, json};

use super::classify::{self, TurnClass};
use super::context::{input_image, user_text};
use super::img;
use super::prompt::{self, OriginalFacts};
use super::relayout_guard::relayout_guard_message;
use super::tools::ToolCtx;
use crate::openai::decision_cost_krw;
use crate::state::lock;

pub(super) const COMPARE_MAX_PX: u32 = 1200;
const DPI_GUARD_LIST: usize = 8;
/// Two checks at most: the second confirms the fix the first asked for, without looping on a stubborn case.
const RELAYOUT_GUARD_CHECKS: u32 = 2;

#[derive(Default)]
pub(super) struct Guards {
    similarity_fails: usize,
    dpi_checked: bool,
    relayout_checks: u32,
}

#[derive(Default)]
pub(super) struct GuardStep {
    pub(super) label: Option<String>,
    pub(super) send_back: Option<Value>,
    pub(super) review_krw: Option<f64>,
    pub(super) escalate: bool,
}

impl Guards {
    pub(super) async fn next(
        &mut self,
        ctx: &ToolCtx,
        facts: Option<&OriginalFacts>,
        class: TurnClass,
        cheap_model: &str,
    ) -> anyhow::Result<GuardStep> {
        let (threshold, max_fails) = guard_limits(lock(&ctx.handle).project.fit_mode);
        if ctx.state.edited() && facts.is_some() && self.similarity_fails < max_fails {
            let (sim, png) = {
                let h = lock(&ctx.handle);
                let f = facts.ok_or_else(|| anyhow!("원본 없음"))?;
                let compared = img::compare_full(&h, &ctx.fonts, f, COMPARE_MAX_PX)?;
                (compared.result.similarity, compared.side_by_side_png)
            };
            if sim < threshold {
                self.similarity_fails += 1;
                return Ok(GuardStep {
                    label: Some(format!(
                        "가드 · 원본과 비교 · 유사도 {:.0}% (더 고침)",
                        sim * 100.0
                    )),
                    send_back: Some(json!({"type": "message", "role": "user", "content": [
                        {"type": "input_text", "text": format!("{} (similarity {:.2})", prompt::GUARD_FAIL_MESSAGE, sim)},
                        input_image(&png)
                    ]})),
                    review_krw: None,
                    escalate: self.similarity_fails == 2,
                });
            }
        }
        if ctx.state.edited() && !self.dpi_checked {
            let short = dpi_guard_message(&lock(&ctx.handle));
            if let Some((count, text)) = short {
                self.dpi_checked = true;
                return Ok(GuardStep {
                    label: Some(format!("가드 · 출력 해상도 · {count}개 부족")),
                    send_back: Some(user_text(&text)),
                    ..Default::default()
                });
            }
        }
        if ctx.state.edited() && self.relayout_checks < RELAYOUT_GUARD_CHECKS {
            let text = {
                let h = lock(&ctx.handle);
                (h.project.fit_mode == FitMode::Relayout && facts.is_some())
                    .then(|| relayout_guard_message(&h, &ctx.fonts))
                    .flatten()
            };
            if let Some(text) = text {
                self.relayout_checks += 1;
                return Ok(GuardStep {
                    label: Some("가드 · 재배치 점검 · 제목 비율·간격 (더 고침)".into()),
                    send_back: Some(user_text(&text)),
                    ..Default::default()
                });
            }
        }
        let rearranged =
            class == TurnClass::Relayout || lock(&ctx.handle).project.fit_mode == FitMode::Relayout;
        if ctx.state.edited() && self.relayout_checks == 0 && rearranged {
            let png = {
                let h = lock(&ctx.handle);
                img::work_image(&h, &ctx.fonts, 1024).and_then(|im| Ok(cimg::to_png_bytes(&im)?))
            };
            let review = match png {
                Ok(png) => classify::review(&ctx.openai, cheap_model, &png).await,
                Err(e) => Err(e),
            };
            match review {
                Ok(r) => {
                    let label = format!(
                        "검수 · 그림으로 답답함 {:.0}%{}",
                        r.crowded * 100.0,
                        if r.failed() { " (더 고침)" } else { "" }
                    );
                    let send_back = r.failed().then(|| {
                        self.relayout_checks = RELAYOUT_GUARD_CHECKS;
                        user_text(&crowded_message(r.crowded))
                    });
                    return Ok(GuardStep {
                        label: Some(label),
                        send_back,
                        review_krw: Some(decision_cost_krw(&r.usage)),
                        escalate: false,
                    });
                }
                Err(e) => log::warn!("image review skipped: {e}"),
            }
        }
        Ok(GuardStep::default())
    }
}

fn crowded_message(probability: f64) -> String {
    format!(
        "[guard] A reviewer looking at the whole work judged it crowded (probability {:.2}): text or boxes sit against the rim, rows have no air between them, or the title is crowded. Give the margins and gaps back — text and its boxes at least 5% of the height from the top and bottom and 10% from the ends, about 4% of the height between rows, boxes hugging their content — then view_work to confirm.",
        probability
    )
}

fn dpi_guard_message(h: &ProjectHandle) -> Option<(usize, String)> {
    let dpi = h.project.target_dpi;
    let warnings = hoist_core::check::export_check(&h.document, dpi);
    if warnings.is_empty() {
        return None;
    }
    let px = |mm: f64, need: f64| (mm / 25.4 * need).ceil() as u64;
    let mut lines: Vec<String> = warnings
        .iter()
        .take(DPI_GUARD_LIST)
        .map(|w| {
            let need = h
                .document
                .layer(&w.layer_id)
                .map(|l| {
                    format!(
                        ", needs {}×{}px",
                        px(l.w_mm, w.required_dpi),
                        px(l.h_mm, w.required_dpi)
                    )
                })
                .unwrap_or_default();
            format!(
                "{} (now {:.0} dpi, required {:.0} dpi{need})",
                w.layer_name, w.effective_dpi, w.required_dpi
            )
        })
        .collect();
    if warnings.len() > DPI_GUARD_LIST {
        lines.push(format!("and {} more", warnings.len() - DPI_GUARD_LIST));
    }
    Some((
        warnings.len(),
        format!(
            "[guard] These layers are below the print resolution requirement (target {dpi:.0} dpi, half of that for backgrounds): {}. Rebuild them with extend_background (larger height_px, max 2160) or generate_image at a larger size, then confirm with export_check.",
            lines.join(", ")
        ),
    ))
}

/// Relayout moves elements on purpose, so pixel similarity to the original is structurally lower.
fn guard_limits(fit: FitMode) -> (f64, usize) {
    match fit {
        FitMode::Relayout => (0.70, 1),
        _ => (0.85, 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_guard_names_short_layers_and_the_pixels_they_need() {
        let root = std::env::temp_dir().join(format!("hoist-dpi-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root)
            .create("dpi", 254.0, 127.0)
            .expect("project");
        assert!(dpi_guard_message(&h).is_none(), "no images, nothing to say");
        let image = |id: &str, natural_w_px: u32| hoist_core::model::Layer {
            id: id.into(),
            name: format!("그림 {id}"),
            parent: None,
            kind: hoist_core::model::LayerKind::Image(hoist_core::model::ImageProps {
                asset: "a.png".into(),
                natural_w_px,
                natural_h_px: natural_w_px / 2,
                ..Default::default()
            }),
            x_mm: 0.0,
            y_mm: 0.0,
            w_mm: 254.0,
            h_mm: 127.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![],
            strokes: vec![],
            effects: vec![],
            strategy: hoist_core::model::Strategy::OriginalCrop,
            strategy_reason: String::new(),
            pinned: vec![],
            source: None,
        };
        h.document.layers = vec![image("sharp", 1000), image("soft", 500)];
        let (count, text) = dpi_guard_message(&h).expect("one short layer");
        assert_eq!(count, 1);
        assert!(
            text.contains("그림 soft (now 50 dpi, required 100 dpi, needs 1000×500px)"),
            "{text}"
        );
        assert!(!text.contains("그림 sharp"), "{text}");
    }

    #[test]
    fn relayout_relaxes_the_guard() {
        assert_eq!(guard_limits(FitMode::Relayout), (0.70, 1));
        assert_eq!(guard_limits(FitMode::ExtendBackground), (0.85, 2));
        assert_eq!(guard_limits(FitMode::Ask), (0.85, 2));
    }
}
