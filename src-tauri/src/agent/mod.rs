pub mod classify;
pub mod img;
pub mod layer_args;
pub mod prompt;
pub mod tools;

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use hoist_core::image as cimg;
use hoist_core::model::*;
use hoist_core::ops::describe;
use hoist_core::render::layer_bounds_mm;
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;

use crate::openai::{ContextOverflow, OpenAI, decision_cost_krw, text_cost_krw};
use crate::popups::NoticeKind;
use crate::state::AppState;
pub use classify::TurnClass;
use prompt::{OriginalFacts, TurnKind, original_facts};
use tools::{ToolCtx, TurnState};

pub struct AgentRun {
    pub running: AtomicBool,
    pub stop: AtomicBool,
    pub stop_notify: Notify,
    pub pending: Mutex<HashMap<String, tokio::sync::oneshot::Sender<String>>>,
}

impl AgentRun {
    pub fn new() -> Self {
        AgentRun {
            running: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            stop_notify: Notify::new(),
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.stop_notify.notify_waiters();
    }

    pub fn answer(&self, question_id: &str, answer: &str) -> bool {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(question_id)
            .is_some_and(|tx| tx.send(answer.to_string()).is_ok())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, specta::Type)]
pub struct SendContext {
    #[serde(default)]
    pub layer_ids: Vec<String>,
    #[serde(default)]
    pub captures: Vec<Capture>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Pane {
    Original,
    Work,
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct Capture {
    pub pane: Pane,
    pub x_mm: f64,
    pub y_mm: f64,
    pub w_mm: f64,
    pub h_mm: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Running,
    Done,
    Failed,
    Stopped,
}

const DPI_GUARD_LIST: usize = 8;

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

/// Relayout turns are judged by design rules, not by similarity. Sizes are read on ink (the real glyph extent)
/// and empty space counts as part of the layout: the title's glyphs take 25–30% of the height and read as one
/// evenly spaced line, text and the boxes that carry it stay clear of the banner's rim, rows keep their
/// distance, boxes hug their content and still pad it, info columns stay within two lines, and glyph
/// proportions stay those of the font.
fn relayout_guard_message(h: &ProjectHandle, fonts: &Fonts) -> Option<String> {
    struct Text<'a> {
        layer: &'a Layer,
        props: &'a TextProps,
        ink: [f64; 4],
        lines: usize,
    }
    let doc = &h.document;
    let height = doc.artboard.height_mm.max(1.0);
    let width = doc.artboard.width_mm.max(1.0);
    let texts: Vec<Text> = doc
        .layers
        .iter()
        .filter(|l| l.visible)
        .filter_map(|layer| match &layer.kind {
            LayerKind::Text(props) if props.font_size_mm > 0.0 => {
                let (ink, lines) = tools::ink_mm(fonts, layer);
                Some(Text {
                    layer,
                    props,
                    ink,
                    lines,
                })
            }
            _ => None,
        })
        .collect();
    let max_size = texts.iter().map(|t| t.props.font_size_mm).fold(0.0, f64::max);
    if max_size <= 0.0 {
        return None;
    }
    let rect = |l: &Layer| [l.x_mm, l.y_mm, l.w_mm, l.h_mm];
    let inside = |b: [f64; 4], c: [f64; 4]| {
        let (cx, cy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
        (c[0]..=c[0] + c[2]).contains(&cx) && (c[1]..=c[1] + c[3]).contains(&cy)
    };
    let overlap_x = |a: [f64; 4], b: [f64; 4]| (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let distance_y = |a: [f64; 4], b: [f64; 4]| (b[1] - (a[1] + a[3])).max(a[1] - (b[1] + b[3]));
    let of_kind = |shape: bool| -> Vec<&Layer> {
        doc.layers
            .iter()
            .filter(|l| {
                l.visible
                    && match l.kind {
                        LayerKind::Shape(_) => shape,
                        LayerKind::Path(_) => !shape,
                        _ => false,
                    }
            })
            .collect()
    };
    let (shapes, paths) = (of_kind(true), of_kind(false));
    let (title, body): (Vec<&Text>, Vec<&Text>) =
        texts.iter().partition(|t| t.props.font_size_mm >= max_size * 0.8);
    let title_box = tools::union_mm(title.iter().map(|t| t.ink))?;
    let ink_h = title.iter().map(|t| t.ink[3]).fold(0.0, f64::max);
    let mut problems = Vec::new();

    let share = ink_h / height;
    if share > 0.32 {
        problems.push(format!(
            "the title's glyphs are {ink_h:.0} mm tall, {:.0}% of the banner height (target 25–30%): make the title smaller and give the height back to the margins and gaps",
            share * 100.0
        ));
    } else if share < 0.24 && title_box[2] < 0.9 * (width - 0.2 * height) {
        problems.push(format!(
            "the title's glyphs are {ink_h:.0} mm tall, only {:.0}% of the banner height, with width to spare (target 25–30%)",
            share * 100.0
        ));
    }
    let mut words = title.clone();
    words.sort_by(|a, b| a.ink[0].total_cmp(&b.ink[0]));
    let gaps: Vec<f64> = words
        .windows(2)
        .map(|p| p[1].ink[0] - (p[0].ink[0] + p[0].ink[2]))
        .collect();
    let (gap_min, gap_max) = gaps
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), g| (lo.min(*g), hi.max(*g)));
    if !gaps.is_empty() && gap_max > ink_h * 0.8 {
        problems.push(format!(
            "title words are {gap_max:.0} mm apart (keep about 0.3× the glyph height, {:.0} mm)",
            ink_h * 0.3
        ));
    }
    if gaps.len() >= 2 && gap_max - gap_min > ink_h * 0.08 {
        problems.push(format!(
            "title words are unevenly spaced (gaps {gap_min:.0} and {gap_max:.0} mm): call arrange_row on them with one gap of about 0.3× the glyph height"
        ));
    }
    let smallest = texts
        .iter()
        .map(|t| t.props.font_size_mm)
        .fold(f64::MAX, f64::min);
    if smallest / height < 0.03 {
        problems.push(format!(
            "the smallest text is {smallest:.0} mm, under 3% of the height ({:.0} mm)",
            height * 0.03
        ));
    }
    for t in body
        .iter()
        .filter(|t| t.ink[1] + t.ink[3] < title_box[1] && t.props.font_size_mm < max_size * 0.3)
    {
        problems.push(format!(
            "'{}' stands over the title at {:.0} mm, under 0.3× the title ({max_size:.0} mm): keep the original's step, an eyebrow is about half the title",
            t.layer.name, t.props.font_size_mm
        ));
    }

    let panels: Vec<&Layer> = shapes.iter().copied().filter(|l| l.w_mm > width * 0.5).collect();
    for panel in panels.iter().filter(|l| l.h_mm > height * 0.33) {
        problems.push(format!(
            "'{}' is {:.0} mm tall, {:.0}% of the banner height: a box is its content plus padding (an info panel is about 26%)",
            panel.name,
            panel.h_mm,
            panel.h_mm / height * 100.0
        ));
    }
    for s in &shapes {
        let LayerKind::Shape(ShapeProps::Rect { rx_mm }) = s.kind else {
            continue;
        };
        if rx_mm < s.h_mm * 0.3 || s.w_mm < width * 0.1 {
            continue;
        }
        let held: Vec<[f64; 4]> = body
            .iter()
            .filter(|t| inside(t.ink, rect(s)))
            .map(|t| t.ink)
            .collect();
        if held.is_empty() {
            continue;
        }
        let inner = shapes
            .iter()
            .filter(|o| o.id != s.id && o.w_mm < s.w_mm)
            .map(|o| rect(o))
            .filter(|b| inside(*b, rect(s)));
        let content = tools::union_mm(held.into_iter().chain(inner)).map_or(0.0, |u| u[2]);
        if content < s.w_mm * 0.6 {
            problems.push(format!(
                "'{}' is {:.0} mm wide but its content is {content:.0} mm: fit_box it around its content",
                s.name, s.w_mm
            ));
        }
    }
    for panel in &panels {
        let held: Vec<&Text> = body
            .iter()
            .copied()
            .filter(|t| inside(t.ink, rect(panel)))
            .collect();
        if held.len() < 3 {
            continue;
        }
        for t in held.iter().filter(|t| t.lines > 2) {
            problems.push(format!(
                "'{}' has {} lines: re-break the info columns to at most 2 lines",
                t.layer.name, t.lines
            ));
        }
        let badges = shapes
            .iter()
            .filter(|o| o.id != panel.id && inside(rect(o), rect(panel)))
            .map(|o| rect(o))
            .filter(|b| held.iter().any(|t| inside(t.ink, *b)));
        let Some(content) = tools::union_mm(held.iter().map(|t| t.ink).chain(badges)) else {
            continue;
        };
        let mut sizes: Vec<f64> = held.iter().map(|t| t.props.font_size_mm).collect();
        sizes.sort_by(f64::total_cmp);
        let body_size = sizes[sizes.len() / 2];
        let pads = [
            ("top", content[1] - panel.y_mm),
            ("bottom", panel.y_mm + panel.h_mm - (content[1] + content[3])),
            ("left", content[0] - panel.x_mm),
            ("right", panel.x_mm + panel.w_mm - (content[0] + content[2])),
        ];
        if let Some((side, pad)) = pads
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, pad)| *pad < body_size * 0.4)
        {
            problems.push(format!(
                "'{}' leaves {pad:.0} mm between its content and its {side} edge: fit_box it with pad_y 0.6× and pad_x 1× the body size ({body_size:.0} mm)",
                panel.name
            ));
        }
        // An info column starts on one vertical line: its badge sits at the top-left of its body.
        let pills: Vec<&Layer> = shapes
            .iter()
            .copied()
            .filter(|o| {
                o.id != panel.id
                    && o.w_mm < width * 0.2
                    && inside(rect(o), rect(panel))
                    && held.iter().any(|t| inside(t.ink, rect(o)))
            })
            .collect();
        let bodies: Vec<&Text> = held
            .iter()
            .copied()
            .filter(|t| !pills.iter().any(|o| inside(t.ink, rect(o))))
            .collect();
        let columns: Vec<(&Layer, &Text)> = pills
            .iter()
            .filter_map(|o| {
                bodies
                    .iter()
                    .filter(|t| t.ink[1] >= o.y_mm + o.h_mm * 0.5 && overlap_x(t.ink, rect(o)) > 0.0)
                    .min_by(|a, b| a.ink[1].total_cmp(&b.ink[1]))
                    .map(|t| (*o, *t))
            })
            .collect();
        let off = columns
            .iter()
            .filter(|(o, t)| (t.ink[0] - o.x_mm).abs() > body_size * 0.25)
            .count();
        if let Some((o, t)) = columns
            .iter()
            .find(|(o, t)| (t.ink[0] - o.x_mm).abs() > body_size * 0.25)
        {
            problems.push(format!(
                "{off} badges do not start where their body starts ('{}' at {:.0} mm, its body at {:.0} mm): body align left and the badge's x on the body's ink left",
                o.name, o.x_mm, t.ink[0]
            ));
        }
        let spread = |v: &[f64]| {
            v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min)
        };
        let badge_tops: Vec<f64> = columns.iter().map(|(o, _)| o.y_mm).collect();
        if badge_tops.len() > 1 && spread(&badge_tops) > body_size * 0.1 {
            problems.push("the info badges are not on one line: give them one y and height".into());
        }
        let body_tops: Vec<f64> = columns.iter().map(|(_, t)| t.ink[1]).collect();
        if body_tops.len() > 1 && spread(&body_tops) > body_size * 0.25 {
            problems.push("the info bodies do not share one top: give them one y".into());
        }
        for t in &bodies {
            let written: Vec<&str> = t.props.text.split('\n').collect();
            if t.lines > written.len() {
                problems.push(format!(
                    "'{}' wrapped on its own into {} lines: write the breaks with \\n and make the box wider than the longest line",
                    t.layer.name, t.lines
                ));
            }
            let last = written
                .last()
                .map_or(0, |l| l.chars().filter(|c| !c.is_whitespace()).count());
            if written.len() > 1 && last <= 1 {
                problems.push(format!(
                    "'{}' ends in a one-character line: break between phrases instead",
                    t.layer.name
                ));
            }
        }
    }
    // Texts that share a box play one role, so they share the fitted scale_x. Texts elsewhere in the same
    // family (a year on its own band) were fitted on their own and may differ.
    let mut widened: std::collections::BTreeMap<&str, (f64, f64, &str)> = std::collections::BTreeMap::new();
    for s in &shapes {
        let mut spread: std::collections::BTreeMap<&str, (f64, f64)> = std::collections::BTreeMap::new();
        for t in body.iter().filter(|t| inside(t.ink, rect(s))) {
            let e = spread
                .entry(t.props.font_family.as_str())
                .or_insert((f64::MAX, f64::MIN));
            *e = (e.0.min(t.props.scale_x), e.1.max(t.props.scale_x));
        }
        for (family, (lo, hi)) in spread {
            if hi - lo > 0.05 && widened.get(family).is_none_or(|w| hi - lo > w.1 - w.0) {
                widened.insert(family, (lo, hi, s.name.as_str()));
            }
        }
    }
    for (family, (lo, hi, name)) in widened {
        problems.push(format!(
            "texts in {family} inside '{name}' use different scale_x ({lo:.2}–{hi:.2}): texts that share a box use one fitted scale_x"
        ));
    }

    // The rim belongs to the hem and the rope eyelets: text and the boxes that carry it stay out of it.
    // Full-bleed bars and bare decorations (leaves, brush strokes without text) may reach the edge.
    let full_bleed: Vec<[f64; 4]> = shapes
        .iter()
        .filter(|s| s.w_mm >= width * 0.95)
        .map(|s| rect(s))
        .collect();
    let carriers: Vec<(&str, [f64; 4])> = texts
        .iter()
        .filter(|t| !full_bleed.iter().any(|f| inside(t.ink, *f)))
        .map(|t| (t.layer.name.as_str(), t.ink))
        .chain(
            shapes
                .iter()
                .filter(|s| s.w_mm < width * 0.95)
                .map(|s| (s.name.as_str(), rect(s))),
        )
        .chain(
            paths
                .iter()
                .filter(|p| texts.iter().any(|t| inside(t.ink, rect(p))))
                .map(|p| (p.name.as_str(), rect(p))),
        )
        .collect();
    let nearest = |distance: &dyn Fn([f64; 4]) -> f64, limit: f64| {
        let close: Vec<(&str, f64)> = carriers
            .iter()
            .map(|(name, b)| (*name, distance(*b)))
            .filter(|(_, d)| *d < limit)
            .collect();
        close
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(name, d)| (name, d, close.len() - 1))
    };
    let more = |n: usize| {
        if n > 0 {
            format!(" (and {n} more)")
        } else {
            String::new()
        }
    };
    for (edge, distance) in [
        ("top", &(|b: [f64; 4]| b[1]) as &dyn Fn([f64; 4]) -> f64),
        ("bottom", &|b: [f64; 4]| height - (b[1] + b[3])),
    ] {
        if let Some((name, d, n)) = nearest(distance, height * 0.04) {
            problems.push(format!(
                "'{name}' is {d:.0} mm from the {edge} edge{}: keep text and its boxes at least {:.0} mm (5% of the height) from the top and bottom",
                more(n),
                height * 0.05
            ));
        }
    }
    if let Some((name, d, n)) = nearest(&|b: [f64; 4]| b[0].min(width - (b[0] + b[2])), height * 0.08) {
        problems.push(format!(
            "'{name}' is {d:.0} mm from the banner's end{}: keep text and its boxes at least {:.0} mm (10% of the height) from the left and right ends",
            more(n),
            height * 0.10
        ));
    }

    // Rows: nothing sits on or against the title's glyphs (a cut-out picture whose loose box only clips a
    // corner, under 15% of its width, is left alone), and boxed rows keep their distance from each other.
    let neighbours = body.iter().map(|t| (t.layer.name.as_str(), t.ink, false)).chain(
        doc.layers
            .iter()
            .filter(|l| {
                l.visible
                    && matches!(
                        l.kind,
                        LayerKind::Shape(_) | LayerKind::Path(_) | LayerKind::Image(_)
                    )
                    && l.w_mm >= width * 0.05
                    && l.w_mm < width * 0.9
            })
            .map(|l| (l.name.as_str(), rect(l), matches!(l.kind, LayerKind::Image(_)))),
    );
    let mut crowding = 0;
    for (name, b, picture) in neighbours {
        if overlap_x(title_box, b) <= b[2] * 0.15 {
            continue;
        }
        let d = distance_y(title_box, b);
        let limit = if picture { -height * 0.02 } else { height * 0.025 };
        if d >= limit || crowding >= 3 {
            continue;
        }
        crowding += 1;
        problems.push(if d < 0.0 {
            format!(
                "'{name}' overlaps the title by {:.0} mm vertically: rows must not overlap, move it clear or shrink the band",
                -d
            )
        } else {
            format!(
                "'{name}' is only {d:.0} mm from the title's glyphs: leave about 4% of the height ({:.0} mm) between rows",
                height * 0.04
            )
        });
    }
    let boxed: Vec<&Layer> = shapes
        .iter()
        .copied()
        .filter(|s| s.w_mm >= width * 0.1 && body.iter().any(|t| inside(t.ink, rect(s))))
        .collect();
    for (i, a) in boxed.iter().enumerate() {
        for b in &boxed[i + 1..] {
            let (ra, rb) = (rect(a), rect(b));
            if inside(ra, rb) || inside(rb, ra) || overlap_x(ra, rb) <= ra[2].min(rb[2]) * 0.15 {
                continue;
            }
            let d = distance_y(ra, rb);
            if d < height * 0.025 {
                problems.push(format!(
                    "'{}' and '{}' are {d:.0} mm apart: leave about 4% of the height ({:.0} mm) between rows",
                    a.name,
                    b.name,
                    height * 0.04
                ));
            }
        }
    }
    if problems.is_empty() {
        return None;
    }
    Some(format!(
        "[guard] Relayout check failed: {}. Re-flow following <relayout>: keep the margins, gaps and padding, place title words and row items with arrange_row, size every band, pill and panel to its content with fit_box, then view_work to confirm.",
        problems.join("; ")
    ))
}

/// The system prompt depends on the fit mode, and a first turn starts before the user has chosen one: the
/// relayout skill is only part of a prompt built after the answer. Rebuilt when the mode changes and at no
/// other time, so the cached prefix stays the same for the rest of the turn.
struct TurnPrompt {
    fit: FitMode,
    text: String,
}

impl TurnPrompt {
    fn build(h: &ProjectHandle, facts: Option<&OriginalFacts>, kind: TurnKind) -> Self {
        TurnPrompt {
            fit: h.project.fit_mode,
            text: prompt::instructions(
                &h.project,
                &prompt::PromptFacts {
                    original: facts.copied(),
                    layer_count: h.document.layers.len(),
                    memory: h.memory().unwrap_or_default(),
                },
                kind,
            ),
        }
    }

    fn refresh(&mut self, h: &ProjectHandle, facts: Option<&OriginalFacts>, kind: TurnKind) {
        if h.project.fit_mode != self.fit {
            *self = Self::build(h, facts, kind);
        }
    }
}

/// Relayout moves elements on purpose, so pixel similarity to the original is structurally lower.
fn guard_limits(fit: hoist_core::model::FitMode) -> (f64, usize) {
    match fit {
        hoist_core::model::FitMode::Relayout => (0.70, 1),
        _ => (0.85, 2),
    }
}

fn mark_stopped(action: &mut ActionItem, label: &str) {
    action.status = ActionStatus::Stopped;
    action.label = format!("{label} · 중단됨");
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct ActionItem {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub status: ActionStatus,
    #[serde(default)]
    pub round: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct Report {
    pub original_png: String,
    pub work_png: String,
    pub overlap: f64,
    pub similarity: f64,
    pub min_dpi: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_min_dpi: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi_ok: Option<bool>,
    pub promoted: usize,
    pub summary: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct Question {
    pub id: String,
    pub text: String,
    pub options: Vec<String>,
    #[serde(default)]
    pub allow_free_text: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_png: Option<String>,
    #[serde(default = "auto_label")]
    pub auto_label: String,
}

pub const AUTO_LABEL: &str = "에이전트가 알아서";
/// What the UI sends when the user picks `auto_label`.
pub const AUTO_ANSWER: &str = "__auto__";

fn auto_label() -> String {
    AUTO_LABEL.into()
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatItem {
    User(UserChat),
    Agent(Box<AgentChat>),
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct UserChat {
    pub id: String,
    pub ts: String,
    pub text: String,
    #[serde(default)]
    pub context: SendContext,
    #[serde(default)]
    pub edit_summary: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct AgentChat {
    pub id: String,
    pub ts: String,
    pub text: String,
    #[serde(default)]
    pub actions: Vec<ActionItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<Report>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<Question>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<Question>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub turn_krw: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub thinking_sec: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl AgentChat {
    /// `question`/`answer` keep the first entry so chats written before multi-question asks still read the same.
    pub fn record_question(&mut self, q: Question) {
        if self.question.is_none() {
            self.question = Some(q.clone());
        }
        self.questions.push(q);
        self.answers.resize(self.questions.len(), None);
    }

    pub fn has_question(&self, question_id: &str) -> bool {
        self.questions.iter().any(|q| q.id == question_id)
            || self.question.as_ref().is_some_and(|q| q.id == question_id)
    }

    pub fn record_answer(&mut self, question_id: &str, answer: &str) {
        if self.question.as_ref().is_some_and(|q| q.id == question_id) {
            self.answer = Some(answer.to_string());
        }
        if let Some(i) = self.questions.iter().position(|q| q.id == question_id) {
            self.answers.resize(self.questions.len(), None);
            self.answers[i] = Some(answer.to_string());
        }
    }
}

pub fn chat_items(h: &ProjectHandle) -> Vec<ChatItem> {
    h.chat()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| match serde_json::from_value(v) {
            Ok(item) => Some(item),
            Err(e) => {
                log::warn!("skipping unreadable chat item: {e}");
                None
            }
        })
        .collect()
}

pub fn update_agent_chat(
    h: &ProjectHandle,
    matches: impl Fn(&AgentChat) -> bool,
    update: impl FnOnce(&mut AgentChat),
) {
    let found =
        h.chat()
            .unwrap_or_default()
            .into_iter()
            .rev()
            .find_map(|v| match serde_json::from_value(v) {
                Ok(ChatItem::Agent(item)) if matches(&item) => Some(item),
                _ => None,
            });
    let Some(mut item) = found else {
        return;
    };
    update(&mut item);
    let id = item.id.clone();
    if let Ok(v) = serde_json::to_value(ChatItem::Agent(item)) {
        let _ = h.update_chat(&id, &v);
    }
}

#[derive(Serialize, Clone, Debug, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEventKind {
    TurnStart,
    ThinkingStart {
        round: usize,
    },
    Compacted {
        reason: String,
    },
    Usage {
        turn_krw: f64,
        project_krw: f64,
        text_krw: f64,
        image_krw: f64,
    },
    TextDelta {
        delta: String,
    },
    Text {
        text: String,
    },
    Action {
        action: ActionItem,
    },
    DocumentChanged {
        rev: u64,
    },
    Report {
        report: Report,
    },
    Question {
        question: Question,
    },
    TurnEnd {
        stopped: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Serialize, Clone, Debug, specta::Type)]
pub struct AgentEvent {
    pub project_id: String,
    pub turn: String,
    #[serde(flatten)]
    pub event: AgentEventKind,
}

#[derive(Clone)]
struct Emit {
    channel: Channel<AgentEvent>,
    project_id: String,
    turn: String,
}

impl Emit {
    fn send(&self, event: AgentEventKind) {
        let _ = self.channel.send(AgentEvent {
            project_id: self.project_id.clone(),
            turn: self.turn.clone(),
            event,
        });
    }
}

fn items_path(h: &ProjectHandle) -> std::path::PathBuf {
    h.dir.join("agent_items.json")
}

fn agent_state_path(h: &ProjectHandle) -> std::path::PathBuf {
    h.dir.join("agent_state.json")
}

fn load_items(h: &ProjectHandle) -> Vec<Value> {
    std::fs::read(items_path(h))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_items(h: &ProjectHandle, items: &[Value]) {
    let _ = std::fs::write(items_path(h), serde_json::to_vec(items).unwrap_or_default());
}

fn load_agent_state(h: &ProjectHandle) -> Value {
    std::fs::read(agent_state_path(h))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(json!({}))
}

fn save_agent_state(h: &ProjectHandle, v: &Value) {
    let _ = std::fs::write(agent_state_path(h), serde_json::to_vec(v).unwrap_or_default());
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

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

pub fn edit_summary(h: &ProjectHandle, since_seq: u64) -> Vec<String> {
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

/// A turn runs for minutes and the window may be in the tray: say when it ends or needs an answer.
fn notify(app: &AppHandle, handle: &Arc<Mutex<ProjectHandle>>, kind: NoticeKind) {
    let project = img::lock(handle).project.clone();
    crate::popups::notify(
        app,
        crate::popups::Notice {
            project_id: project.id,
            project_name: project.name,
            kind,
        },
    );
}

pub async fn run_turn(
    app: AppHandle,
    project_id: String,
    text: String,
    ctx_in: SendContext,
    kind: TurnKind,
    on_event: Channel<AgentEvent>,
) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    let key = settings
        .openai_key()
        .ok_or_else(|| anyhow!("설정에서 OpenAI API 키를 먼저 넣어 주세요"))?;
    let openai = OpenAI::new(key)?;
    let handle = state.handle(&project_id)?;
    let fonts = state.fonts.clone();
    let run = {
        let mut agents = state.agents.lock().unwrap_or_else(|e| e.into_inner());
        agents
            .entry(project_id.clone())
            .or_insert_with(|| Arc::new(AgentRun::new()))
            .clone()
    };
    if run.running.swap(true, Ordering::SeqCst) {
        anyhow::bail!("에이전트가 이미 작업 중이에요");
    }
    run.stop.store(false, Ordering::SeqCst);
    let turn = uuid::Uuid::new_v4().simple().to_string();
    let emit = Emit {
        channel: on_event,
        project_id: project_id.clone(),
        turn: turn.clone(),
    };
    let result = run_turn_inner(
        &app,
        &state,
        &settings,
        openai,
        handle.clone(),
        fonts,
        run.clone(),
        &emit,
        text,
        ctx_in,
        kind,
    )
    .await;
    run.running.store(false, Ordering::SeqCst);
    run.pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
    notify(
        &app,
        &handle,
        if result.is_ok() && !run.stop.load(Ordering::SeqCst) {
            NoticeKind::Done
        } else {
            NoticeKind::Stopped
        },
    );
    if let Err(e) = &result {
        emit.send(AgentEventKind::Error {
            message: e.to_string(),
        });
        emit.send(AgentEventKind::TurnEnd { stopped: true });
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_turn_inner(
    app: &AppHandle,
    state: &AppState,
    settings: &crate::settings::Settings,
    openai: OpenAI,
    handle: Arc<Mutex<ProjectHandle>>,
    fonts: Arc<Fonts>,
    run: Arc<AgentRun>,
    emit: &Emit,
    text: String,
    ctx_in: SendContext,
    mut kind: TurnKind,
) -> anyhow::Result<()> {
    let turn = emit.turn.clone();
    let user_item_id = uuid::Uuid::new_v4().simple().to_string();
    // The agent chat item shares the turn id, so the UI's live item and the stored item are the same entry.
    let agent_item_id = turn.clone();
    let (classify_input, layer_count, has_original) = {
        let h = img::lock(&handle);
        (
            classify::input(&h, &text, &ctx_in.layer_ids),
            h.document.layers.len(),
            !h.project.originals.is_empty(),
        )
    };
    let mut class = TurnClass::from_keywords(&text);
    let mut class_krw = 0.0;
    let mut class_label = None;
    if kind != TurnKind::ReplaceOriginal && layer_count > 0 {
        let asked_at = std::time::Instant::now();
        match classify::classify(&openai, &settings.cheap_model, &classify_input).await {
            Ok(v) => {
                class_krw = decision_cost_krw(&v.usage);
                if let Some(c) = v.class {
                    class = c;
                }
                log::info!(
                    "turn class {} ({} {:.2}) in {} ms",
                    class.label(),
                    v.choice,
                    v.probability,
                    asked_at.elapsed().as_millis()
                );
                class_label = Some(format!(
                    "분류 · {} · {} {:.0}%",
                    class.label(),
                    if v.class.is_some() {
                        "확률"
                    } else {
                        "확신 없음, 키워드로"
                    },
                    v.probability * 100.0
                ));
            }
            Err(e) => {
                log::warn!("turn classification skipped: {e}");
                class_label = Some(format!("분류 · 실패해 기존 경로 · {}", short(&e.to_string())));
            }
        }
        kind = turn_kind(layer_count, has_original, class);
    }
    let (facts, mut items, last_seq, summary_lines, user_parts, project_name) = {
        let h = img::lock(&handle);
        let facts = original_facts(&h.project);
        let items = load_items(&h);
        let st = load_agent_state(&h);
        let last_seq = st["last_seq"].as_u64().unwrap_or(0);
        let summary = edit_summary(&h, last_seq);
        let parts = build_user_parts(&h, &fonts, facts.as_ref(), &text, &ctx_in, &summary, kind, class)?;
        (facts, items, last_seq, summary, parts, h.project.name.clone())
    };
    let _ = last_seq;
    let user_chat = ChatItem::User(UserChat {
        id: user_item_id,
        ts: now(),
        text,
        context: ctx_in,
        edit_summary: summary_lines,
    });
    let agent_chat = ChatItem::Agent(Box::new(AgentChat {
        id: agent_item_id.clone(),
        ts: now(),
        text: String::new(),
        actions: Vec::new(),
        report: None,
        question: None,
        answer: None,
        questions: Vec::new(),
        answers: Vec::new(),
        turn_krw: 0.0,
        thinking_sec: 0.0,
    }));
    {
        let h = img::lock(&handle);
        h.append_chat(&serde_json::to_value(&user_chat)?)?;
        h.append_chat(&serde_json::to_value(&agent_chat)?)?;
    }
    emit.send(AgentEventKind::TurnStart);
    items.push(json!({"type": "message", "role": "user", "content": user_parts}));

    let turn_state = Arc::new(TurnState {
        asked: AtomicBool::new(false),
        must_ask_first: kind == TurnKind::ReplaceOriginal,
        fit_pending: AtomicBool::new(prompt::needs_fit_answer(
            &img::lock(&handle).project,
            facts.as_ref(),
        )),
        edits: AtomicUsize::new(0),
        promoted: AtomicUsize::new(0),
        cost_krw: Mutex::new(0.0),
        memory_dirty: AtomicBool::new(false),
        round: AtomicUsize::new(0),
        memo: tools::ReadMemo::default(),
        plan: Mutex::new(None),
        plan_pending: AtomicBool::new(matches!(kind, TurnKind::Reconstruct | TurnKind::Scratch)),
        wording_edits: AtomicUsize::new(0),
    });
    let question_sink = {
        let emit = emit.clone();
        let handle = handle.clone();
        let agent_item_id = agent_item_id.clone();
        let app = app.clone();
        Arc::new(move |question: Question| {
            emit.send(AgentEventKind::Question {
                question: question.clone(),
            });
            notify(&app, &handle, NoticeKind::Question);
            let h = img::lock(&handle);
            update_agent_chat(&h, |c| c.id == agent_item_id, |c| c.record_question(question));
        })
    };
    let tool_ctx = ToolCtx {
        handle: handle.clone(),
        fonts: fonts.clone(),
        openai: openai.clone(),
        image_model: settings.image_model.clone(),
        turn: turn.clone(),
        facts: original_facts(&img::lock(&handle).project),
        run: run.clone(),
        state: turn_state.clone(),
        on_question: question_sink,
        models_dir: state.models_dir.clone(),
    };
    let heavy = matches!(kind, TurnKind::Reconstruct | TurnKind::Scratch);
    // A simple edit stays on the agent model at low effort: it already drives the strict layer schemas
    // reliably, and with the layer records in the first request a wrong edit costs more than the tokens.
    let (mut model, mut effort) = if heavy {
        (settings.heavy_model.clone(), "high")
    } else if class == TurnClass::Simple {
        (settings.agent_model.clone(), "low")
    } else {
        (settings.agent_model.clone(), "medium")
    };
    let mut planned = false;
    let mut instructions = TurnPrompt::build(&img::lock(&handle), facts.as_ref(), kind);
    let mut tools = tools::definitions();
    let mut strict_tools = true;
    let mut full_text = String::new();
    let mut actions: Vec<ActionItem> = Vec::new();
    let mut total_cost = 0.0;
    let mut guard_fails = 0;
    let mut dpi_guarded = false;
    let mut relayout_guarded = false;
    let mut stopped = false;
    let mut rounds: usize = 0;
    let mut image_outputs_ok = true;
    let mut images_billed = 0.0;
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut last_input_tokens = 0u64;
    let mut overflow_stage = 0;
    let mut thinking = std::time::Duration::ZERO;
    if let Some(label) = class_label {
        let aid = push_action(&mut actions, &label, None, ActionStatus::Done, 0);
        emit.send(AgentEventKind::Action {
            action: actions[aid].clone(),
        });
        bill_usage(
            class_krw,
            &mut total_cost,
            &mut images_billed,
            &turn_state,
            &handle,
            emit,
        );
    }

    let loop_result: anyhow::Result<()> = async {
    'outer: loop {
        if rounds > MAX_ROUNDS {
            full_text.push_str("\n\n(도구 호출 한도에 닿아 여기서 멈췄어요.)");
            break;
        }
        if run.stop.load(Ordering::SeqCst) {
            stopped = true;
            break;
        }
        rounds += 1;
        turn_state.round.store(rounds, Ordering::SeqCst);
        instructions.refresh(&img::lock(&handle), facts.as_ref(), kind);
        if heavy
            && !planned
            && turn_state
                .plan
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        {
            planned = true;
            model = settings.agent_model.clone();
            effort = "medium";
        }
        let max_output_tokens = if effort == "high" { 32_000 } else { 12_000 };
        log::debug!("round {rounds} model={model} effort={effort} max_output_tokens={max_output_tokens}");
        if last_input_tokens > DEMOTE_AT_INPUT_TOKENS {
            demote_seen_images(
                &mut items,
                KEEP_IMAGES,
                &handle,
                facts.as_ref(),
                &seen,
                &turn_state.memo,
            );
        }
        let body = json!({
            "model": model,
            "instructions": instructions.text,
            "input": items,
            "tools": tools,
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "reasoning": {"effort": effort},
            "prompt_cache_key": emit.project_id,
            "store": false,
            "include": ["reasoning.encrypted_content"],
            "max_output_tokens": max_output_tokens
        });
        emit.send(AgentEventKind::ThinkingStart { round: rounds });
        let mut round_text = String::new();
        let asked_at = std::time::Instant::now();
        let response = openai
            .responses_stream(body, |d| {
                round_text.push_str(d);
                emit.send(AgentEventKind::TextDelta { delta: d.to_string() });
            })
            .await;
        thinking += asked_at.elapsed();
        let out = match response {
            Ok(o) => {
                overflow_stage = 0;
                o
            }
            Err(e) => {
                if e.downcast_ref::<ContextOverflow>().is_some() {
                    if overflow_stage == 0
                        && demote_seen_images(
                            &mut items,
                            KEEP_IMAGES_ON_OVERFLOW,
                            &handle,
                            facts.as_ref(),
                            &seen,
                            &turn_state.memo,
                        ) > 0
                    {
                        overflow_stage = 1;
                        continue;
                    }
                    if overflow_stage < 2 {
                        let compact_instructions =
                            format!("{}\n\n{}", instructions.text, prompt::COMPACT_INSTRUCTIONS);
                        let compacted = openai.compact(&model, &items, &compact_instructions).await?;
                        if !compacted.is_empty() {
                            items = compacted;
                            overflow_stage = 2;
                            emit.send(AgentEventKind::Compacted {
                                reason: "컨텍스트 한도 초과".into(),
                            });
                            continue;
                        }
                    }
                    return Err(e);
                }
                let message = e.to_string();
                if strict_tools && (message.contains("Invalid schema") || message.contains("param: tools[")) {
                    strict_tools = false;
                    layer_args::strip_strict(&mut tools);
                    continue;
                }
                if image_outputs_ok && message.contains("function_call_output") {
                    image_outputs_ok = false;
                    items = strip_image_outputs(std::mem::take(&mut items));
                    continue;
                }
                return Err(e);
            }
        };
        last_input_tokens = out.usage.input_tokens;
        let round_text_krw = text_cost_krw(&model, &out.usage);
        if !round_text.is_empty() {
            if !full_text.is_empty() {
                full_text.push_str("\n\n");
            }
            full_text.push_str(&round_text);
        }
        let mut calls: Vec<Value> = Vec::new();
        for item in &out.output {
            items.push(item.clone());
            if item["type"] == "function_call" {
                calls.push(item.clone());
            }
        }
        if calls.is_empty() {
            bill_usage(
                round_text_krw,
                &mut total_cost,
                &mut images_billed,
                &turn_state,
                &handle,
                emit,
            );
            if turn_state.wording_only() {
                break;
            }
            let (threshold, max_guards) = guard_limits(img::lock(&handle).project.fit_mode);
            if turn_state.edits.load(Ordering::SeqCst) > 0 && facts.is_some() && guard_fails < max_guards {
                let (sim, png) = {
                    let h = img::lock(&handle);
                    let f = facts.as_ref().ok_or_else(|| anyhow!("원본 없음"))?;
                    let fc = img::compare_full(&h, &fonts, f, 1200)?;
                    (fc.result.similarity, fc.side_by_side_png)
                };
                if sim < threshold {
                    guard_fails += 1;
                    if guard_fails == 2 {
                        model = settings.heavy_model.clone();
                        effort = "high";
                    }
                    let aid = push_action(
                        &mut actions,
                        &format!("가드 · 원본과 비교 · 유사도 {:.0}% (더 고침)", sim * 100.0),
                        None,
                        ActionStatus::Done,
                        rounds,
                    );
                    emit.send(AgentEventKind::Action {
                        action: actions[aid].clone(),
                    });
                    items.push(json!({"type": "message", "role": "user", "content": [
                        {"type": "input_text", "text": format!("{} (similarity {:.2})", prompt::GUARD_FAIL_MESSAGE, sim)},
                        {"type": "input_image", "image_url": img::data_url(&png), "detail": "high"}
                    ]}));
                    continue 'outer;
                }
            }
            if turn_state.edits.load(Ordering::SeqCst) > 0 && !dpi_guarded {
                let short = dpi_guard_message(&img::lock(&handle));
                if let Some((count, text)) = short {
                    dpi_guarded = true;
                    let aid = push_action(
                        &mut actions,
                        &format!("가드 · 출력 해상도 · {count}개 부족"),
                        None,
                        ActionStatus::Done,
                        rounds,
                    );
                    emit.send(AgentEventKind::Action {
                        action: actions[aid].clone(),
                    });
                    items.push(json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}));
                    continue 'outer;
                }
            }
            if turn_state.edits.load(Ordering::SeqCst) > 0 && !relayout_guarded {
                let text = {
                    let h = img::lock(&handle);
                    (h.project.fit_mode == hoist_core::FitMode::Relayout && facts.is_some())
                        .then(|| relayout_guard_message(&h, &fonts))
                        .flatten()
                };
                if let Some(text) = text {
                    relayout_guarded = true;
                    let aid = push_action(
                        &mut actions,
                        "가드 · 재배치 점검 · 제목 비율·간격 (더 고침)",
                        None,
                        ActionStatus::Done,
                        rounds,
                    );
                    emit.send(AgentEventKind::Action {
                        action: actions[aid].clone(),
                    });
                    items.push(json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}));
                    continue 'outer;
                }
            }
            let rearranged = class == TurnClass::Relayout
                || img::lock(&handle).project.fit_mode == hoist_core::FitMode::Relayout;
            if turn_state.edits.load(Ordering::SeqCst) > 0 && !relayout_guarded && rearranged {
                let png = {
                    let h = img::lock(&handle);
                    img::work_image(&h, &fonts, 1024).and_then(|im| Ok(cimg::to_png_bytes(&im)?))
                };
                let review = match png {
                    Ok(png) => classify::review(&openai, &settings.cheap_model, &png).await,
                    Err(e) => Err(e),
                };
                match review {
                    Ok(r) => {
                        bill_usage(
                            decision_cost_krw(&r.usage),
                            &mut total_cost,
                            &mut images_billed,
                            &turn_state,
                            &handle,
                            emit,
                        );
                        let aid = push_action(
                            &mut actions,
                            &format!(
                                "검수 · 그림으로 답답함 {:.0}%{}",
                                r.crowded * 100.0,
                                if r.failed() { " (더 고침)" } else { "" }
                            ),
                            None,
                            ActionStatus::Done,
                            rounds,
                        );
                        emit.send(AgentEventKind::Action {
                            action: actions[aid].clone(),
                        });
                        if r.failed() {
                            relayout_guarded = true;
                            items.push(json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": format!(
                                "[guard] A reviewer looking at the whole work judged it crowded (probability {:.2}): text or boxes sit against the rim, rows have no air between them, or the title is crowded. Give the margins and gaps back — text and its boxes at least 5% of the height from the top and bottom and 10% from the ends, about 4% of the height between rows, boxes hugging their content — then view_work to confirm.",
                                r.crowded
                            )}]}));
                            continue 'outer;
                        }
                    }
                    Err(e) => log::warn!("image review skipped: {e}"),
                }
            }
            break;
        }
        let mut pending: Vec<PendingCall> = Vec::new();
        let mut jobs: Vec<(String, Value)> = Vec::new();
        for call in calls {
            let name = call["name"].as_str().unwrap_or("").to_string();
            let call_id = call["call_id"].as_str().unwrap_or("").to_string();
            let args: Value =
                serde_json::from_str(call["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}));
            let label = tools::label_for(&name, &args);
            let detail = {
                let h = img::lock(&handle);
                tools::detail_for(&name, &args, &h.document, tool_ctx.facts.as_ref())
            };
            seen.insert(call_id.clone(), rounds);
            let aid = push_action(
                &mut actions,
                &label,
                detail.clone(),
                ActionStatus::Running,
                rounds,
            );
            emit.send(AgentEventKind::Action {
                action: actions[aid].clone(),
            });
            pending.push(PendingCall {
                call_id,
                name: name.clone(),
                label,
                detail,
                aid,
            });
            jobs.push((name, args));
        }
        let ctx = tool_ctx.clone();
        let tools_started = std::time::Instant::now();
        let (results, interrupted) = run_calls(
            jobs,
            &run,
            |(name, _): &(String, Value)| tools::phase(name),
            move |(name, args): (String, Value)| {
                let ctx = ctx.clone();
                async move { tools::execute(&ctx, &name, args).await }
            },
            |i, result| {
                let p = &pending[i];
                log::debug!(
                    "round {rounds} tool {} finished {} ms after the round's tools started",
                    p.name,
                    tools_started.elapsed().as_millis()
                );
                match result {
                    Ok(r) => {
                        actions[p.aid].status = ActionStatus::Done;
                        if r.cached {
                            actions[p.aid].detail = Some(match &p.detail {
                                Some(d) => format!("{d} · 이미 본 결과"),
                                None => "이미 본 결과".into(),
                            });
                        }
                        if tools_edit(&p.name) {
                            let rev = img::lock(&handle).document.rev;
                            emit.send(AgentEventKind::DocumentChanged { rev });
                        }
                    }
                    Err(_) if run.stop.load(Ordering::SeqCst) => mark_stopped(&mut actions[p.aid], &p.label),
                    Err(e) => {
                        actions[p.aid].status = ActionStatus::Failed;
                        actions[p.aid].label = format!("{} · {}", p.label, short(&e.to_string()));
                    }
                }
                emit.send(AgentEventKind::Action {
                    action: actions[p.aid].clone(),
                });
                let h = img::lock(&handle);
                update_agent_chat(&h, |c| c.id == agent_item_id, |c| c.actions = actions.clone());
            },
        )
        .await;
        for (p, result) in pending.iter().zip(results) {
            let output = match result {
                Some(Ok(r)) => tool_output(&p.call_id, r, image_outputs_ok),
                Some(Err(e)) => {
                    json!({"type": "function_call_output", "call_id": p.call_id, "output": json!({"error": e.to_string()}).to_string()})
                }
                None => {
                    if interrupted {
                        mark_stopped(&mut actions[p.aid], &p.label);
                    } else {
                        actions[p.aid].status = ActionStatus::Failed;
                    }
                    emit.send(AgentEventKind::Action {
                        action: actions[p.aid].clone(),
                    });
                    let why = if interrupted {
                        "사용자가 작업을 중단했어요"
                    } else {
                        "도구 실행이 중간에 끊겼어요"
                    };
                    json!({"type": "function_call_output", "call_id": p.call_id, "output": json!({"error": why}).to_string()})
                }
            };
            items.push(output);
        }
        {
            let h = img::lock(&handle);
            update_agent_chat(&h, |c| c.id == agent_item_id, |c| c.actions = actions.clone());
        }
        bill_usage(
            round_text_krw,
            &mut total_cost,
            &mut images_billed,
            &turn_state,
            &handle,
            emit,
        );
        if interrupted {
            stopped = true;
            break 'outer;
        }
    }
    Ok(())
    }
    .await;
    if let Err(e) = &loop_result {
        log::error!("agent turn failed: {e:#}");
        for a in actions.iter_mut() {
            if matches!(a.status, ActionStatus::Running) {
                a.status = ActionStatus::Failed;
                a.label = format!("{} · 오류", a.label);
                emit.send(AgentEventKind::Action { action: a.clone() });
            }
        }
        if !full_text.is_empty() {
            full_text.push_str(
                "

",
            );
        }
        full_text.push_str(&format!("오류로 작업을 멈췄어요: {}", short(&e.to_string())));
    }
    bill_usage(
        0.0,
        &mut total_cost,
        &mut images_billed,
        &turn_state,
        &handle,
        emit,
    );

    let report = if turn_state.edits.load(Ordering::SeqCst) > 0 {
        let h = img::lock(&handle);
        match facts.as_ref() {
            Some(f) => {
                let fc = img::compare_full(&h, &fonts, f, 1200)?;
                let dpi = hoist_core::check::dpi_summary(&h.document, h.project.target_dpi);
                let min_dpi = dpi.min_dpi.unwrap_or(h.project.target_dpi);
                let background = dpi
                    .background_min_dpi
                    .map(|b| format!(" · 배경 {}dpi", b.round()))
                    .unwrap_or_default();
                let promoted = turn_state.promoted.load(Ordering::SeqCst);
                let summary = format!(
                    "유사도 {:.0}% · 겹침 {:.0}% · 실물 {}dpi{background} {} · 방식 바꿈 {}",
                    fc.result.similarity * 100.0,
                    fc.result.overlap * 100.0,
                    min_dpi.round(),
                    if dpi.ok { "충족" } else { "부족" },
                    promoted
                );
                Some(Report {
                    original_png: img::data_url(&fc.original_png),
                    work_png: img::data_url(&fc.work_png),
                    overlap: fc.result.overlap,
                    similarity: fc.result.similarity,
                    min_dpi,
                    background_min_dpi: dpi.background_min_dpi,
                    dpi_ok: Some(dpi.ok),
                    promoted,
                    summary,
                })
            }
            None => {
                let png = img::thumbnail_png(&h, &fonts)?;
                Some(Report {
                    original_png: String::new(),
                    work_png: img::data_url(&png),
                    overlap: 0.0,
                    similarity: 0.0,
                    min_dpi: h.project.target_dpi,
                    background_min_dpi: None,
                    dpi_ok: None,
                    promoted: 0,
                    summary: "시안".into(),
                })
            }
        }
    } else {
        None
    };
    if let Some(r) = &report {
        emit.send(AgentEventKind::Report { report: r.clone() });
    }
    if stopped && full_text.is_empty() {
        full_text = "작업을 중단했어요.".into();
    }
    emit.send(AgentEventKind::Text {
        text: full_text.clone(),
    });
    {
        let mut h = img::lock(&handle);
        update_agent_chat(
            &h,
            |c| c.id == agent_item_id,
            |c| {
                c.text = full_text.clone();
                c.actions = actions;
                c.report = report;
                c.turn_krw = total_cost;
                c.thinking_sec = thinking.as_secs_f64();
            },
        );
        save_items(&h, &items);
        if turn_state.edits.load(Ordering::SeqCst) > 0 {
            let thumb = img::thumbnail_png(&h, &fonts).ok();
            let summary = full_text
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("에이전트 작업")
                .chars()
                .take(40)
                .collect::<String>();
            let _ = h.snapshot(
                Actor::Agent,
                &format!("{summary} · 에이전트"),
                Some(turn.clone()),
                thumb.as_deref(),
            );
            if let Some(t) = &thumb {
                let _ = h.write_thumbnail(t);
            }
        }
        let mut st = load_agent_state(&h);
        st["last_seq"] = json!(h.last_seq());
        st["last_turn"] = json!(turn);
        save_agent_state(&h, &st);
    }
    let _ = project_name;
    emit.send(AgentEventKind::TurnEnd { stopped });
    loop_result?;
    Ok(())
}

fn tools_edit(name: &str) -> bool {
    matches!(
        name,
        "add_layer"
            | "update_layer"
            | "delete_layer"
            | "move_layer"
            | "arrange_row"
            | "fit_box"
            | "trace_region"
    )
}

fn short(s: &str) -> String {
    s.chars().take(60).collect()
}

fn push_action(
    actions: &mut Vec<ActionItem>,
    label: &str,
    detail: Option<String>,
    status: ActionStatus,
    round: usize,
) -> usize {
    actions.push(ActionItem {
        id: uuid::Uuid::new_v4().simple().to_string(),
        label: label.to_string(),
        detail,
        status,
        round: u32::try_from(round).unwrap_or(u32::MAX),
    });
    actions.len() - 1
}

const MAX_ROUNDS: usize = 200;

struct PendingCall {
    call_id: String,
    name: String,
    label: String,
    detail: Option<String>,
    aid: usize,
}

type CallResults = Vec<Option<anyhow::Result<tools::ToolResult>>>;

/// Phase 0 calls run first, one at a time in call order; phase 1 calls then run together; higher phases run
/// last, one at a time, lower phase first and in call order within a phase.
async fn run_calls<J, F, Fut>(
    jobs: Vec<J>,
    run: &AgentRun,
    phase: impl Fn(&J) -> usize,
    exec: F,
    mut done: impl FnMut(usize, &anyhow::Result<tools::ToolResult>),
) -> (CallResults, bool)
where
    F: Fn(J) -> Fut,
    Fut: Future<Output = anyhow::Result<tools::ToolResult>> + Send + 'static,
{
    let mut results: CallResults = jobs.iter().map(|_| None).collect();
    if run.stop.load(Ordering::SeqCst) {
        return (results, true);
    }
    let mut tagged: Vec<(usize, usize, J)> = jobs
        .into_iter()
        .enumerate()
        .map(|(i, j)| (phase(&j), i, j))
        .collect();
    tagged.sort_by_key(|(p, i, _)| (*p, *i));
    let (first, rest): (Vec<_>, Vec<_>) = tagged.into_iter().partition(|(p, _, _)| *p == 0);
    let (together, last): (Vec<_>, Vec<_>) = rest.into_iter().partition(|(p, _, _)| *p == 1);
    if run_in_order(first, run, &exec, &mut results, &mut done).await {
        return (results, true);
    }
    let mut set = tokio::task::JoinSet::new();
    for (_, i, job) in together {
        let task = exec(job);
        set.spawn(async move { (i, task.await) });
    }
    loop {
        let stop = run.stop_notify.notified();
        tokio::pin!(stop);
        stop.as_mut().enable();
        if run.stop.load(Ordering::SeqCst) {
            set.abort_all();
            return (results, true);
        }
        tokio::select! {
            joined = set.join_next() => match joined {
                None => break,
                Some(Ok((i, result))) => {
                    done(i, &result);
                    results[i] = Some(result);
                }
                Some(Err(e)) => log::warn!("tool task ended abnormally: {e}"),
            },
            () = &mut stop => {}
        }
    }
    let stopped = run_in_order(last, run, &exec, &mut results, &mut done).await;
    (results, stopped)
}

/// Returns true when the user stopped the turn.
async fn run_in_order<J, F, Fut>(
    jobs: Vec<(usize, usize, J)>,
    run: &AgentRun,
    exec: &F,
    results: &mut CallResults,
    done: &mut impl FnMut(usize, &anyhow::Result<tools::ToolResult>),
) -> bool
where
    F: Fn(J) -> Fut,
    Fut: Future<Output = anyhow::Result<tools::ToolResult>>,
{
    for (_, i, job) in jobs {
        let stop = run.stop_notify.notified();
        tokio::pin!(stop);
        stop.as_mut().enable();
        if run.stop.load(Ordering::SeqCst) {
            return true;
        }
        tokio::select! {
            result = exec(job) => {
                done(i, &result);
                results[i] = Some(result);
            }
            () = &mut stop => return true,
        }
    }
    false
}

fn bill_usage(
    text_krw: f64,
    total: &mut f64,
    images_billed: &mut f64,
    turn_state: &TurnState,
    handle: &Arc<Mutex<ProjectHandle>>,
    emit: &Emit,
) {
    let images = *turn_state.cost_krw.lock().unwrap_or_else(|e| e.into_inner());
    let image_krw = images - *images_billed;
    *images_billed = images;
    if text_krw + image_krw <= 0.0 {
        return;
    }
    *total += text_krw + image_krw;
    let project = {
        let mut h = img::lock(handle);
        if let Err(e) = h.add_usage(text_krw, image_krw) {
            log::warn!("project usage not saved: {e}");
        }
        h.project.clone()
    };
    emit.send(AgentEventKind::Usage {
        turn_krw: *total,
        project_krw: project.usage_krw,
        text_krw: project.usage_text_krw,
        image_krw: project.usage_image_krw,
    });
}

const DEMOTE_AT_INPUT_TOKENS: u64 = 300_000;
const KEEP_IMAGES: usize = 4;
const KEEP_IMAGES_ON_OVERFLOW: usize = 2;

fn demote_images(
    items: &mut [Value],
    keep: usize,
    placeholder: impl Fn(Option<(&str, &Value, &str)>) -> String,
) -> usize {
    let calls: HashMap<String, (String, Value)> = items
        .iter()
        .filter(|it| it["type"] == "function_call")
        .filter_map(|it| {
            let id = it["call_id"].as_str()?.to_string();
            let name = it["name"].as_str().unwrap_or("").to_string();
            let args = serde_json::from_str(it["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}));
            Some((id, (name, args)))
        })
        .collect();
    let mut slots: Vec<(usize, &'static str, usize, Option<String>)> = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let (field, call_id) = match it["type"].as_str() {
            Some("function_call_output") => ("output", it["call_id"].as_str().map(str::to_string)),
            Some("message") => ("content", None),
            _ => continue,
        };
        if let Some(parts) = it[field].as_array() {
            for (j, part) in parts.iter().enumerate() {
                if part["type"] == "input_image" {
                    slots.push((i, field, j, call_id.clone()));
                }
            }
        }
    }
    let demote = slots.len().saturating_sub(keep);
    for (i, field, j, call_id) in slots.into_iter().take(demote) {
        let call = call_id
            .as_deref()
            .and_then(|id| calls.get(id).map(|(name, args)| (name.as_str(), args, id)));
        items[i][field][j] = json!({"type": "input_text", "text": placeholder(call)});
    }
    demote
}

fn demote_seen_images(
    items: &mut [Value],
    keep: usize,
    handle: &Arc<Mutex<ProjectHandle>>,
    facts: Option<&OriginalFacts>,
    seen: &HashMap<String, usize>,
    memo: &tools::ReadMemo,
) -> usize {
    let h = img::lock(handle);
    let demoted = demote_images(items, keep, |call| {
        match call {
        Some((name, args, call_id)) => {
            let label = tools::label_for(name, args);
            let what = match tools::detail_for(name, args, &h.document, facts) {
                Some(d) => format!("{label} {d}"),
                None => label,
            };
            let when = seen
                .get(call_id)
                .map(|r| format!("seen in round {r}"))
                .unwrap_or_else(|| "seen in an earlier turn".into());
            format!("(image omitted: {what}, {when} — call the same tool again with another region or zoom if you need it)")
        }
        None => "(image omitted: an image attached to the message, already seen — call view_original or view_work if you need it again)".into(),
    }
    });
    if demoted > 0 {
        memo.forget_images();
    }
    demoted
}

fn tool_output(call_id: &str, r: tools::ToolResult, images_ok: bool) -> Value {
    if r.images.is_empty() || !images_ok {
        return json!({"type": "function_call_output", "call_id": call_id, "output": r.text});
    }
    let mut parts = vec![json!({"type": "input_text", "text": r.text})];
    for png in r.images {
        parts.push(json!({"type": "input_image", "image_url": img::data_url(&png), "detail": "high"}));
    }
    json!({"type": "function_call_output", "call_id": call_id, "output": parts})
}

fn strip_image_outputs(items: Vec<Value>) -> Vec<Value> {
    items
        .into_iter()
        .map(|mut it| {
            if it["type"] == "function_call_output" && it["output"].is_array() {
                let text = it["output"]
                    .as_array()
                    .and_then(|a| a.iter().find(|p| p["type"] == "input_text"))
                    .and_then(|p| p["text"].as_str())
                    .unwrap_or("")
                    .to_string();
                it["output"] = json!(text);
            }
            it
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_user_parts(
    h: &ProjectHandle,
    fonts: &Fonts,
    facts: Option<&prompt::OriginalFacts>,
    text: &str,
    ctx: &SendContext,
    summary: &[String],
    kind: TurnKind,
    class: TurnClass,
) -> anyhow::Result<Vec<Value>> {
    let mut parts: Vec<Value> = Vec::new();
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
    if !ctx.layer_ids.is_empty() {
        msg.push_str("\n<selected_layers>\n");
        for id in &ctx.layer_ids {
            if let Some(l) = h.document.layer(id) {
                msg.push_str(&selected_layer_line(l));
            }
        }
        msg.push_str("</selected_layers>\n");
    }
    if class == TurnClass::Simple {
        if ctx.layer_ids.is_empty() {
            msg.push_str(&text_layer_list(&h.document));
        }
        msg.push('\n');
        msg.push_str(prompt::SIMPLE_TURN_NOTE);
        msg.push('\n');
    }
    for c in &ctx.captures {
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
    parts.push(json!({"type": "input_text", "text": msg}));
    for id in &ctx.layer_ids {
        if let Some(l) = h.document.layer(id) {
            let b = layer_bounds_mm(&h.document, l);
            let rect = img::clamp_rect_mm(h, [b[0] - 5.0, b[1] - 5.0, b[2] + 10.0, b[3] + 10.0]);
            if let Ok(im) = img::work_region(&h.document, &h.assets_dir(), fonts, rect, 800) {
                parts.push(json!({"type": "input_image", "image_url": img::data_url(&cimg::to_png_bytes(&im)?), "detail": "high"}));
            }
        }
    }
    for c in &ctx.captures {
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
            img::work_region(&h.document, &h.assets_dir(), fonts, rect, 1000)?
        };
        parts.push(json!({"type": "input_image", "image_url": img::data_url(&cimg::to_png_bytes(&cimg::resize_max(&im, 1000))?), "detail": "high"}));
    }
    if (kind == TurnKind::Reconstruct || kind == TurnKind::ReplaceOriginal)
        && let Some(f) = facts
    {
        let orig = img::original_image(h, f.version)?;
        parts.push(json!({"type": "input_text", "text": format!("<original version=\"{}\"/> full image attached below", f.version)}));
        parts.push(json!({"type": "input_image", "image_url": img::data_url(&cimg::to_png_bytes(&cimg::resize_max(&orig, 1536))?), "detail": "high"}));
    }
    Ok(parts)
}

pub fn spawn_turn(
    app: AppHandle,
    project_id: String,
    text: String,
    ctx: SendContext,
    kind: TurnKind,
    on_event: Channel<AgentEvent>,
) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_turn(app, project_id, text, ctx, kind, on_event).await {
            log::error!("agent turn failed: {e:#}");
        }
    });
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

/// Rebuild only when there is nothing yet or the class says to start over, so copies and templates edit normally.
pub fn turn_kind(layer_count: usize, has_original: bool, class: TurnClass) -> TurnKind {
    if layer_count > 0 && class != TurnClass::Rebuild {
        TurnKind::Normal
    } else if has_original {
        TurnKind::Reconstruct
    } else {
        TurnKind::Scratch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn waited(ms: u64) -> anyhow::Result<tools::ToolResult> {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        Ok(tools::ToolResult {
            text: ms.to_string(),
            images: vec![],
            cached: false,
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn tool_calls_run_at_the_same_time() {
        let run = Arc::new(AgentRun::new());
        let started = std::time::Instant::now();
        let mut finished = Vec::new();
        let (results, stopped) = run_calls(
            vec![300u64, 100, 200],
            &run,
            |_| 1,
            waited,
            |i, _| finished.push(i),
        )
        .await;
        let elapsed = started.elapsed().as_millis();
        assert!(!stopped);
        assert!(elapsed < 450, "calls ran one after another: {elapsed} ms");
        assert_eq!(finished, vec![1, 2, 0]);
        let texts: Vec<String> = results
            .into_iter()
            .map(|r| r.expect("finished").expect("ok").text)
            .collect();
        assert_eq!(texts, vec!["300", "100", "200"]);

        let stopper = run.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            stopper.request_stop();
        });
        let started = std::time::Instant::now();
        let (results, stopped) = run_calls(vec![2000u64, 2000], &run, |_| 1, waited, |_, _| {}).await;
        assert!(stopped);
        assert!(started.elapsed().as_millis() < 1000);
        assert!(results.iter().all(Option::is_none));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn ordered_calls_finish_before_the_rest_start() {
        let run = Arc::new(AgentRun::new());
        let started = std::time::Instant::now();
        let mut finished = Vec::new();
        let (results, stopped) = run_calls(
            vec![101u64, 20, 103, 20],
            &run,
            |ms| usize::from(ms % 2 == 0),
            waited,
            |i, _| finished.push((i, started.elapsed().as_millis())),
        )
        .await;
        assert!(!stopped);
        assert!(results.iter().all(Option::is_some));
        let order: Vec<usize> = finished.iter().map(|f| f.0).collect();
        assert_eq!(order[..2], [0, 2], "{finished:?}");
        let ordered_done = finished[1].1;
        assert!(ordered_done >= 204, "ordered calls overlapped: {finished:?}");
        assert!(
            finished[2..].iter().all(|f| f.1 >= ordered_done + 15),
            "{finished:?}"
        );
        assert!(
            started.elapsed().as_millis() < 400,
            "the rest did not run together"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn layout_tools_run_last_in_their_own_order() {
        let run = Arc::new(AgentRun::new());
        let jobs = [
            "fit_box",
            "add_layer",
            "arrange_row",
            "set_plan",
            "add_layer",
            "arrange_row",
        ];
        let mut finished = Vec::new();
        let (results, stopped) = run_calls(
            jobs.iter().enumerate().map(|(i, n)| (i, *n)).collect(),
            &run,
            |(_, name): &(usize, &str)| tools::phase(name),
            |(i, name): (usize, &str)| async move {
                waited(match (i, name) {
                    (1, _) => 40,
                    (_, "add_layer") => 10,
                    _ => 5,
                })
                .await
            },
            |i, _| finished.push(i),
        )
        .await;
        assert!(!stopped && results.iter().all(Option::is_some));
        assert_eq!(finished, [3, 4, 1, 2, 5, 0], "{finished:?}");
    }

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
    fn the_prompt_gains_the_relayout_skill_once_the_fit_mode_is_answered() {
        let root = std::env::temp_dir().join(format!("hoist-prompt-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root)
            .create("prompt", 6000.0, 900.0)
            .expect("project");
        let facts = OriginalFacts {
            version: 1,
            width_px: 1774,
            height_px: 470,
            mm_per_px: 1.9,
            offset_x_mm: 1301.0,
            offset_y_mm: 0.0,
        };
        let mut prompt = TurnPrompt::build(&h, Some(&facts), TurnKind::Reconstruct);
        assert!(
            !prompt.text.contains("\n<relayout>\n"),
            "an undecided fit mode carries no relayout skill"
        );
        let undecided = prompt.text.clone();
        prompt.refresh(&h, Some(&facts), TurnKind::Reconstruct);
        assert_eq!(
            prompt.text, undecided,
            "the same fit mode keeps the cached prompt"
        );
        h.set_fit_mode(FitMode::Relayout).expect("fit mode");
        prompt.refresh(&h, Some(&facts), TurnKind::Reconstruct);
        assert!(
            prompt.text.contains("\n<relayout>\n") && prompt.text.contains("Fit mode: relayout"),
            "the answer brings the relayout skill into the same turn"
        );
    }

    #[test]
    fn relayout_guard_judges_ink_air_and_boxes() {
        let root = std::env::temp_dir().join(format!("hoist-relayout-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root)
            .create("relayout", 6000.0, 900.0)
            .expect("project");
        let layer = |id: &str, kind: hoist_core::model::LayerKind, rect: [f64; 4]| hoist_core::model::Layer {
            id: id.into(),
            name: id.into(),
            parent: None,
            kind,
            x_mm: rect[0],
            y_mm: rect[1],
            w_mm: rect[2],
            h_mm: rect[3],
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
        let text = |id: &str, size: f64, rect: [f64; 4]| {
            layer(
                id,
                hoist_core::model::LayerKind::Text(hoist_core::model::TextProps {
                    text: id.into(),
                    font_family: "Pretendard".into(),
                    font_size_mm: size,
                    ..Default::default()
                }),
                rect,
            )
        };
        let shape = |id: &str, rx_mm: f64, rect: [f64; 4]| {
            layer(
                id,
                hoist_core::model::LayerKind::Shape(hoist_core::model::ShapeProps::Rect { rx_mm }),
                rect,
            )
        };
        let fonts = Fonts::new(Some(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"),
        ));
        assert!(
            relayout_guard_message(&h, &fonts).is_none(),
            "no text, nothing to judge"
        );
        let ink = |h: &ProjectHandle, id: &str| tools::ink_mm(&fonts, h.document.layer(id).expect("layer")).0;
        let set = |h: &mut ProjectHandle, id: &str, edit: &dyn Fn(&mut hoist_core::model::Layer)| {
            edit(h.document.layer_mut(id).expect("layer"));
        };
        let size_of = |h: &mut ProjectHandle, id: &str, size: f64| {
            if let Some(hoist_core::model::LayerKind::Text(t)) = h.document.layer_mut(id).map(|l| &mut l.kind)
            {
                t.font_size_mm = size;
            }
        };

        // A calm layout: title ink near 27% of the height, a pill under it, a padded panel with three columns.
        h.document.layers = vec![
            text("청전동", 280.0, [1000.0, 150.0, 1300.0, 400.0]),
            text("주민역량강화", 280.0, [2400.0, 150.0, 2600.0, 400.0]),
            shape("알약", 50.0, [2000.0, 525.0, 600.0, 100.0]),
            text("건강한 일상 함께하는 청전동", 60.0, [2100.0, 540.0, 1200.0, 75.0]),
            shape("board", 0.0, [800.0, 660.0, 4400.0, 195.0]),
            text("칸 0", 45.0, [900.0, 700.0, 900.0, 110.0]),
            text("칸 1", 45.0, [2400.0, 700.0, 900.0, 110.0]),
            text("칸 2", 45.0, [3900.0, 700.0, 900.0, 110.0]),
        ];
        let title_ink = ink(&h, "청전동");
        assert!(
            (0.24..0.32).contains(&(title_ink[3] / 900.0)),
            "the fixture's title ink is {} mm",
            title_ink[3]
        );
        let second = ink(&h, "주민역량강화")[0];
        set(&mut h, "주민역량강화", &|l| {
            l.x_mm += title_ink[0] + title_ink[2] + 100.0 - second
        });
        let label = ink(&h, "건강한 일상 함께하는 청전동");
        set(&mut h, "알약", &|l| {
            (l.x_mm, l.w_mm) = (label[0] - 40.0, label[2] + 80.0)
        });
        assert_eq!(relayout_guard_message(&h, &fonts), None, "a calm layout passes");

        let calm = h.document.layers.clone();
        let body_ink = ink(&h, "칸 0");
        h.document
            .layers
            .push(shape("머리표", 20.0, [body_ink[0] + 40.0, 672.0, 200.0, 60.0]));
        h.document
            .layers
            .push(text("표", 45.0, [body_ink[0] + 60.0, 675.0, 100.0, 55.0]));
        let msg = relayout_guard_message(&h, &fonts).expect("a badge off its body's left line");
        assert!(msg.contains("do not start where their body starts"), "{msg}");
        set(&mut h, "머리표", &|l| l.x_mm = body_ink[0]);
        let msg = relayout_guard_message(&h, &fonts).unwrap_or_default();
        assert!(!msg.contains("do not start where"), "{msg}");
        let reword = |h: &mut ProjectHandle, wording: &str, w: f64| {
            let l = h.document.layer_mut("칸 0").expect("layer");
            l.w_mm = w;
            if let hoist_core::model::LayerKind::Text(t) = &mut l.kind {
                t.text = wording.into();
            }
        };
        reword(&mut h, "제천시노인종합복지관 4층", 300.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a line that wrapped on its own");
        assert!(msg.contains("wrapped on its own"), "{msg}");
        reword(&mut h, "복지관 4\n층", 900.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a stranded character");
        assert!(msg.contains("one-character line"), "{msg}");
        h.document.layers = calm.clone();

        h.document
            .layers
            .push(text("연도", 60.0, [2400.0, 45.0, 600.0, 75.0]));
        let msg = relayout_guard_message(&h, &fonts).expect("an eyebrow far under the title");
        assert!(msg.contains("stands over the title"), "{msg}");
        h.document.layers = calm;

        size_of(&mut h, "청전동", 420.0);
        size_of(&mut h, "주민역량강화", 420.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a title that fills the banner");
        assert!(
            msg.contains("the title's glyphs are") && msg.contains("target 25–30%"),
            "{msg}"
        );
        size_of(&mut h, "청전동", 200.0);
        size_of(&mut h, "주민역량강화", 200.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a small title with width to spare");
        assert!(msg.contains("with width to spare"), "{msg}");
        size_of(&mut h, "청전동", 280.0);
        size_of(&mut h, "주민역량강화", 280.0);

        set(&mut h, "주민역량강화", &|l| l.x_mm += 400.0);
        let msg = relayout_guard_message(&h, &fonts).expect("title words far apart");
        assert!(msg.contains("title words are") && msg.contains("apart"), "{msg}");
        set(&mut h, "주민역량강화", &|l| l.x_mm -= 400.0);

        let against = title_ink[1] + title_ink[3] + 5.0;
        set(&mut h, "알약", &|l| l.y_mm = against);
        set(&mut h, "건강한 일상 함께하는 청전동", &|l| {
            l.y_mm = against + 15.0
        });
        let msg = relayout_guard_message(&h, &fonts).expect("a band against the title");
        assert!(
            msg.contains("'알약' is only") && msg.contains("from the title's glyphs"),
            "{msg}"
        );
        set(&mut h, "알약", &|l| l.y_mm = 525.0);
        set(&mut h, "건강한 일상 함께하는 청전동", &|l| {
            l.y_mm = 540.0
        });

        set(&mut h, "알약", &|l| l.w_mm = 1500.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a pill longer than its text");
        assert!(
            msg.contains("'알약' is 1500 mm wide but its content is") && msg.contains("fit_box it"),
            "{msg}"
        );
        set(&mut h, "알약", &|l| l.w_mm = label[2] + 80.0);

        set(&mut h, "board", &|l| l.y_mm = 640.0);
        let msg = relayout_guard_message(&h, &fonts).expect("rows too close");
        assert!(
            msg.contains("'알약' and 'board' are") || msg.contains("'board' and '알약' are"),
            "{msg}"
        );
        set(&mut h, "board", &|l| l.y_mm = 660.0);

        set(&mut h, "board", &|l| l.h_mm = 225.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a panel on the bottom edge");
        assert!(msg.contains("'board' is 15 mm from the bottom edge"), "{msg}");
        set(&mut h, "board", &|l| l.h_mm = 195.0);

        set(&mut h, "칸 0", &|l| l.y_mm = 662.0);
        let msg = relayout_guard_message(&h, &fonts).expect("text against the panel's edge");
        assert!(
            msg.contains("'board' leaves") && msg.contains("its top edge"),
            "{msg}"
        );
        set(&mut h, "칸 0", &|l| l.y_mm = 700.0);

        set(&mut h, "칸 1", &|l| l.w_mm = 200.0);
        if let Some(hoist_core::model::LayerKind::Text(t)) = h.document.layer_mut("칸 1").map(|l| &mut l.kind)
        {
            t.text = "교육일정은 십일월 구일부터 십이월 십사일까지".into();
        }
        let msg = relayout_guard_message(&h, &fonts).expect("a column wraps into many lines");
        assert!(
            msg.contains("'칸 1' has") && msg.contains("at most 2 lines"),
            "{msg}"
        );
        if let Some(hoist_core::model::LayerKind::Text(t)) = h.document.layer_mut("칸 1").map(|l| &mut l.kind)
        {
            t.text = "칸 1".into();
        }
        set(&mut h, "칸 1", &|l| l.w_mm = 900.0);

        let squeeze = |h: &mut ProjectHandle, scale_x: f64| {
            if let Some(hoist_core::model::LayerKind::Text(t)) =
                h.document.layer_mut("칸 0").map(|l| &mut l.kind)
            {
                t.scale_x = scale_x;
            }
        };
        squeeze(&mut h, 1.2);
        let msg = relayout_guard_message(&h, &fonts).expect("widened letters");
        assert!(
            msg.contains("texts in Pretendard inside 'board' use different scale_x (1.00–1.20)"),
            "{msg}"
        );
        squeeze(&mut h, 1.03);
        assert_eq!(
            relayout_guard_message(&h, &fonts),
            None,
            "a small difference in scale_x passes"
        );

        // A year on a brush stroke: the stroke carries text, so it may not sit in the top rim either; its
        // scale_x was fitted on its own and may differ from the columns.
        let mut year = text("연도", 85.0, [2700.0, 50.0, 600.0, 90.0]);
        if let hoist_core::model::LayerKind::Text(t) = &mut year.kind {
            t.scale_x = 1.2;
        }
        h.document.layers.push(year);
        h.document.layers.push(layer(
            "붓띠",
            hoist_core::model::LayerKind::Path(hoist_core::model::PathProps {
                d: "M 0 0 L 800 0 L 800 90 L 0 90 Z".into(),
                fill_rule: Default::default(),
            }),
            [2600.0, 50.0, 800.0, 90.0],
        ));
        assert_eq!(
            relayout_guard_message(&h, &fonts),
            None,
            "a text fitted on its own, clear of the rim"
        );
        set(&mut h, "붓띠", &|l| l.y_mm = 20.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a band in the top rim");
        assert!(msg.contains("'붓띠' is 20 mm from the top edge"), "{msg}");
    }

    #[test]
    fn answers_land_on_their_question_and_the_first_stays_legacy() {
        let q = |id: &str| Question {
            id: id.into(),
            text: id.into(),
            options: vec![],
            allow_free_text: true,
            image_png: None,
            auto_label: AUTO_LABEL.into(),
        };
        let mut chat: AgentChat =
            serde_json::from_value(json!({"id": "a", "ts": "t", "text": ""})).expect("chat");
        chat.record_question(q("q1"));
        chat.record_question(q("q2"));
        assert!(chat.has_question("q2") && !chat.has_question("q3"));
        chat.record_answer("q2", "두 번째");
        assert_eq!(chat.answers, [None, Some("두 번째".to_string())]);
        assert_eq!(chat.answer, None);
        chat.record_answer("q1", "첫 번째");
        assert_eq!(chat.answer.as_deref(), Some("첫 번째"));
        assert_eq!(chat.question.as_ref().map(|q| q.id.as_str()), Some("q1"));

        let run = AgentRun::new();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        run.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert("q9".into(), tx);
        assert!(!run.answer("q8", "x"), "unknown question");
        assert!(run.answer("q9", "y"));
        assert_eq!(rx.try_recv().ok().as_deref(), Some("y"));
    }

    #[test]
    fn layers_make_an_edit_turn_unless_the_class_starts_over() {
        let by_words =
            |n: usize, original: bool, text: &str| turn_kind(n, original, TurnClass::from_keywords(text));
        assert_eq!(
            by_words(46, true, "전화번호를 043-123-4567로 바꿔줘"),
            TurnKind::Normal
        );
        assert_eq!(by_words(46, false, "제목을 키워줘"), TurnKind::Normal);
        assert_eq!(by_words(0, true, "만들어줘"), TurnKind::Reconstruct);
        assert_eq!(by_words(0, false, "가을 축제 현수막"), TurnKind::Scratch);
        assert_eq!(by_words(46, true, "처음부터 다시 해줘"), TurnKind::Reconstruct);
        assert_eq!(by_words(12, false, "다시 만들어 주세요"), TurnKind::Scratch);
        assert_eq!(turn_kind(46, true, TurnClass::Rebuild), TurnKind::Reconstruct);
        assert_eq!(turn_kind(46, false, TurnClass::Rebuild), TurnKind::Scratch);
        assert_eq!(turn_kind(46, true, TurnClass::Simple), TurnKind::Normal);
        assert_eq!(turn_kind(46, true, TurnClass::Relayout), TurnKind::Normal);
        assert_eq!(
            turn_kind(0, true, TurnClass::Simple),
            TurnKind::Reconstruct,
            "nothing to edit yet"
        );
    }

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
        let text_of = |parts: &[Value]| parts[0]["text"].as_str().unwrap_or("").to_string();
        let simple = build_user_parts(
            &h,
            &fonts,
            None,
            "주민사기강화로 바꿔",
            &ctx,
            &[],
            TurnKind::Normal,
            TurnClass::Simple,
        )
        .expect("parts");
        let msg = text_of(&simple);
        assert!(
            msg.contains("\"text\":\"주민역량강화\"") && msg.contains("\"font_size_mm\":300.0"),
            "the text layer's record travels with the message: {msg}"
        );
        assert!(
            msg.contains("- 붓 (붓) · 100,200 1578×306 mm (long record: get_layer for the rest)"),
            "a long path keeps to one line: {msg}"
        );
        assert!(msg.contains("<turn class=\"simple\">"), "{msg}");
        let normal = build_user_parts(
            &h,
            &fonts,
            None,
            "주민사기강화로 바꿔",
            &ctx,
            &[],
            TurnKind::Normal,
            TurnClass::Normal,
        )
        .expect("parts");
        let msg = text_of(&normal);
        assert!(
            msg.contains("\"text\":\"주민역량강화\"") && !msg.contains("<turn class="),
            "{msg}"
        );
    }

    #[test]
    fn relayout_relaxes_the_guard() {
        use hoist_core::model::FitMode;
        assert_eq!(guard_limits(FitMode::Relayout), (0.70, 1));
        assert_eq!(guard_limits(FitMode::ExtendBackground), (0.85, 2));
        assert_eq!(guard_limits(FitMode::Ask), (0.85, 2));
    }

    #[test]
    fn stopped_calls_are_not_failures() {
        let mut action = ActionItem {
            id: "a".into(),
            label: "레이어 추가 · 제목".into(),
            detail: None,
            status: ActionStatus::Running,
            round: 1,
        };
        mark_stopped(&mut action, "레이어 추가 · 제목");
        let v = serde_json::to_value(&action).expect("json");
        assert_eq!(v["status"], "stopped");
        assert_eq!(v["label"], "레이어 추가 · 제목 · 중단됨");
    }

    #[test]
    fn demote_images_keeps_the_latest_and_every_text() {
        let image = |n: u8| json!({"type": "input_image", "image_url": format!("data:image/png;base64,{n}"), "detail": "high"});
        let mut items = vec![
            json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "원본입니다"}, image(0)]}),
            json!({"type": "function_call", "call_id": "c1", "name": "view_original", "arguments": "{\"max_px\":1536}"}),
            json!({"type": "function_call_output", "call_id": "c1", "output": [{"type": "input_text", "text": "v1 1774x470"}, image(1)]}),
            json!({"type": "function_call", "call_id": "c2", "name": "compare", "arguments": "{}"}),
            json!({"type": "function_call_output", "call_id": "c2", "output": [{"type": "input_text", "text": "similarity 0.8"}, image(2)]}),
            json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "확인했어요"}]}),
            json!({"type": "function_call_output", "call_id": "c3", "output": [{"type": "input_text", "text": "w 900px"}, image(3)]}),
        ];
        let texts = |items: &[Value]| -> Vec<String> {
            items
                .iter()
                .flat_map(|it| {
                    it["output"]
                        .as_array()
                        .or_else(|| it["content"].as_array())
                        .cloned()
                        .unwrap_or_default()
                })
                .filter(|p| p["type"] != "input_image")
                .filter_map(|p| p["text"].as_str().map(str::to_string))
                .collect()
        };
        let before = texts(&items);

        let demoted = demote_images(&mut items, 2, |call| {
            format!("생략 {}", call.map(|c| c.0).unwrap_or("메시지"))
        });

        assert_eq!(demoted, 2);
        let after = texts(&items);
        for t in &before {
            assert!(after.contains(t), "text lost: {t}");
        }
        assert!(after.contains(&"생략 메시지".to_string()));
        assert!(after.contains(&"생략 view_original".to_string()));
        let left: Vec<&str> = items
            .iter()
            .flat_map(|it| {
                it["output"]
                    .as_array()
                    .or_else(|| it["content"].as_array())
                    .into_iter()
                    .flatten()
            })
            .filter(|p| p["type"] == "input_image")
            .filter_map(|p| p["image_url"].as_str())
            .collect();
        assert_eq!(left, vec!["data:image/png;base64,2", "data:image/png;base64,3"]);
        assert_eq!(demote_images(&mut items, 2, |_| String::new()), 0);
    }

    #[test]
    fn chat_items_keep_their_stored_shape() {
        let stored = [
            json!({"id": "u1", "role": "user", "ts": "2026-11-05T00:00:00Z", "text": "hi", "context": {"layer_ids": ["L1"], "captures": [{"pane": "work", "x_mm": 1.0, "y_mm": 2.0, "w_mm": 3.0, "h_mm": 4.0}]}, "edit_summary": ["a"]}),
            json!({"id": "a1", "role": "agent", "ts": "2026-11-05T00:00:00Z", "text": "", "actions": []}),
            json!({"id": "a2", "role": "agent", "ts": "2026-11-05T00:00:00Z", "text": "done", "actions": [{"id": "x", "label": "l", "status": "done", "round": 2}], "question": {"id": "q", "text": "?", "options": ["a", "b"]}, "answer": "a", "report": {"original_png": "", "work_png": "", "overlap": 0.5, "similarity": 0.9, "min_dpi": 100.0, "promoted": 1, "summary": "s"}}),
        ];
        for v in stored {
            let item: ChatItem = serde_json::from_value(v.clone()).expect("parse stored chat item");
            let mut expected = v.clone();
            if let Some(q) = expected.get_mut("question") {
                q["allow_free_text"] = json!(false);
                q["auto_label"] = json!(AUTO_LABEL);
            }
            assert_eq!(serde_json::to_value(item).expect("serialize chat item"), expected);
        }
        let costed = json!({"id": "a3", "role": "agent", "ts": "t", "text": "", "actions": [], "turn_krw": 1049.5, "thinking_sec": 312.0});
        let item: ChatItem = serde_json::from_value(costed.clone()).expect("turn summary");
        assert_eq!(serde_json::to_value(item).expect("serialize"), costed);
        let legacy: ActionItem = serde_json::from_value(json!({"id": "x", "label": "l", "status": "done"}))
            .expect("action without round");
        assert_eq!(legacy.round, 0);
    }
}
