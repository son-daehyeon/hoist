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

use crate::openai::{ContextOverflow, OpenAI, text_cost_krw};
use crate::state::AppState;
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

/// Relayout turns are judged by design rules, not by similarity: the title must dominate the height and read
/// as one evenly spaced line, the smallest text must stay legible, boxes must hug their content, info columns
/// stay within two lines, glyph proportions stay those of the font, and nothing may overlap the title's glyphs.
fn relayout_guard_message(h: &ProjectHandle, fonts: &Fonts) -> Option<String> {
    let doc = &h.document;
    let height = doc.artboard.height_mm.max(1.0);
    let width = doc.artboard.width_mm.max(1.0);
    let texts: Vec<(&Layer, &TextProps, [f64; 4], usize)> = doc
        .layers
        .iter()
        .filter(|l| l.visible)
        .filter_map(|l| match &l.kind {
            LayerKind::Text(t) if t.font_size_mm > 0.0 => {
                let (ink, lines) = tools::ink_mm(fonts, l);
                Some((l, t, ink, lines))
            }
            _ => None,
        })
        .collect();
    let max_size = texts.iter().map(|t| t.1.font_size_mm).fold(0.0, f64::max);
    if max_size <= 0.0 {
        return None;
    }
    let is_title = |t: &TextProps| t.font_size_mm >= max_size * 0.8;
    let center_in = |b: [f64; 4], s: &Layer| {
        let (cx, cy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
        (s.x_mm..=s.x_mm + s.w_mm).contains(&cx) && (s.y_mm..=s.y_mm + s.h_mm).contains(&cy)
    };
    let shapes: Vec<&Layer> = doc
        .layers
        .iter()
        .filter(|l| l.visible && matches!(l.kind, LayerKind::Shape(_)))
        .collect();
    let mut problems = Vec::new();
    let share = max_size / height;
    if share < 0.40 {
        problems.push(format!(
            "the largest text is {:.0} mm, only {:.0}% of the banner height (target 40–55%)",
            max_size,
            share * 100.0
        ));
    }
    let mut title: Vec<_> = texts.iter().filter(|t| is_title(t.1)).collect();
    title.sort_by(|a, b| a.0.x_mm.total_cmp(&b.0.x_mm));
    for pair in title.windows(2) {
        let gap = pair[1].0.x_mm - (pair[0].0.x_mm + pair[0].0.w_mm);
        if gap > max_size * 0.8 {
            problems.push(format!(
                "title words '{}' and '{}' are {:.0} mm apart (keep 0.3–0.5× the letter height, about {:.0} mm)",
                pair[0].0.name,
                pair[1].0.name,
                gap,
                max_size * 0.4
            ));
        }
    }
    title.sort_by(|a, b| a.2[0].total_cmp(&b.2[0]));
    let gaps: Vec<f64> = title
        .windows(2)
        .map(|p| p[1].2[0] - (p[0].2[0] + p[0].2[2]))
        .collect();
    let (gap_min, gap_max) = gaps
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), g| (lo.min(*g), hi.max(*g)));
    if gaps.len() >= 2 && gap_max - gap_min > max_size * 0.08 {
        problems.push(format!(
            "title words are unevenly spaced (gaps {gap_min:.0} and {gap_max:.0} mm): call arrange_row on them with one gap of about 0.3× the letter height"
        ));
    }
    let smallest = texts.iter().map(|t| t.1.font_size_mm).fold(f64::MAX, f64::min);
    if smallest / height < 0.03 {
        problems.push(format!(
            "the smallest text is {smallest:.0} mm, under 3% of the height ({:.0} mm)",
            height * 0.03
        ));
    }
    let tallest_box = shapes
        .iter()
        .filter(|l| l.w_mm > width * 0.5)
        .map(|l| l.h_mm)
        .fold(0.0, f64::max);
    if tallest_box > max_size * 1.3 {
        problems.push(format!(
            "a full-width box is {tallest_box:.0} mm tall while the title letters are {max_size:.0} mm: shrink the box to its content and give the height to the title"
        ));
    }
    for s in &shapes {
        let LayerKind::Shape(ShapeProps::Rect { rx_mm }) = s.kind else {
            continue;
        };
        if rx_mm < s.h_mm * 0.3 || s.w_mm < width * 0.1 {
            continue;
        }
        let held: Vec<[f64; 4]> = texts
            .iter()
            .filter(|t| !is_title(t.1) && center_in(t.2, s))
            .map(|t| t.2)
            .collect();
        if held.is_empty() {
            continue;
        }
        let inner = shapes
            .iter()
            .filter(|o| o.id != s.id && o.w_mm < s.w_mm)
            .map(|o| [o.x_mm, o.y_mm, o.w_mm, o.h_mm])
            .filter(|b| center_in(*b, s));
        let content = tools::union_mm(held.into_iter().chain(inner)).map_or(0.0, |u| u[2]);
        if content < s.w_mm * 0.6 {
            problems.push(format!(
                "'{}' is {:.0} mm wide but its content is {content:.0} mm: fit_box it around its content",
                s.name, s.w_mm
            ));
        }
    }
    for panel in shapes.iter().filter(|l| l.w_mm > width * 0.5) {
        let held: Vec<_> = texts
            .iter()
            .filter(|t| !is_title(t.1) && center_in(t.2, panel))
            .collect();
        if held.len() < 3 {
            continue;
        }
        for t in held.iter().filter(|t| t.3 > 2) {
            problems.push(format!(
                "'{}' has {} lines: re-break the info columns to at most 2 lines",
                t.0.name, t.3
            ));
        }
    }
    let mut widths: std::collections::BTreeMap<&str, (f64, f64)> = std::collections::BTreeMap::new();
    for t in texts.iter().filter(|t| !is_title(t.1)) {
        let e = widths
            .entry(t.1.font_family.as_str())
            .or_insert((f64::MAX, f64::MIN));
        *e = (e.0.min(t.1.scale_x), e.1.max(t.1.scale_x));
    }
    for (family, (lo, hi)) in widths {
        if hi - lo > 0.05 {
            problems.push(format!(
                "texts in {family} use different scale_x ({lo:.2}–{hi:.2}): use the fitted scale_x for every text of that role"
            ));
        }
    }
    // Anything at least 5% of the width that intrudes into the title's glyph box (bands, badges, other text);
    // a cut-out picture whose loose box only clips a corner (under 15% of its width) is left alone.
    let others: Vec<(&str, [f64; 4])> = texts
        .iter()
        .filter(|t| !is_title(t.1))
        .map(|t| (t.0.name.as_str(), t.2))
        .chain(
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
                .map(|l| (l.name.as_str(), [l.x_mm, l.y_mm, l.w_mm, l.h_mm])),
        )
        .collect();
    let mut overlaps = 0;
    for t in &title {
        let tb = t.2;
        for (name, ob) in &others {
            let v = (tb[1] + tb[3]).min(ob[1] + ob[3]) - tb[1].max(ob[1]);
            let hz = (tb[0] + tb[2]).min(ob[0] + ob[2]) - tb[0].max(ob[0]);
            if v > height * 0.02 && hz > ob[2] * 0.15 && overlaps < 3 {
                overlaps += 1;
                problems.push(format!(
                    "'{name}' overlaps the title '{}' by {v:.0} mm vertically: rows must not overlap, move it clear or shrink the band",
                    t.0.name
                ));
            }
        }
    }
    if problems.is_empty() {
        return None;
    }
    Some(format!(
        "[guard] Relayout check failed: {}. Re-flow following <relayout>: place title words and row items with arrange_row, size every band, pill and panel to its content with fit_box, keep rows clear of each other, then view_work to confirm.",
        problems.join("; ")
    ))
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
                    format!("{p} {} → {}", show(old.pointer(&ptr)), show(patch.pointer(&ptr)))
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
    state: &AppState,
    settings: &crate::settings::Settings,
    openai: OpenAI,
    handle: Arc<Mutex<ProjectHandle>>,
    fonts: Arc<Fonts>,
    run: Arc<AgentRun>,
    emit: &Emit,
    text: String,
    ctx_in: SendContext,
    kind: TurnKind,
) -> anyhow::Result<()> {
    let turn = emit.turn.clone();
    let user_item_id = uuid::Uuid::new_v4().simple().to_string();
    // The agent chat item shares the turn id, so the UI's live item and the stored item are the same entry.
    let agent_item_id = turn.clone();
    let (facts, mut items, last_seq, summary_lines, user_parts, project_name) = {
        let h = img::lock(&handle);
        let facts = original_facts(&h.project);
        let items = load_items(&h);
        let st = load_agent_state(&h);
        let last_seq = st["last_seq"].as_u64().unwrap_or(0);
        let summary = edit_summary(&h, last_seq);
        let parts = build_user_parts(&h, &fonts, facts.as_ref(), &text, &ctx_in, &summary, kind)?;
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
    });
    let question_sink = {
        let emit = emit.clone();
        let handle = handle.clone();
        let agent_item_id = agent_item_id.clone();
        Arc::new(move |question: Question| {
            emit.send(AgentEventKind::Question {
                question: question.clone(),
            });
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
    let mut model = if heavy {
        settings.heavy_model.clone()
    } else {
        settings.agent_model.clone()
    };
    let mut effort = if heavy { "high" } else { "medium" };
    let mut planned = false;
    let instructions = {
        let h = img::lock(&handle);
        prompt::instructions(
            &h.project,
            &prompt::PromptFacts {
                original: facts.as_ref().map(|f| prompt::OriginalFacts { ..*f }),
                layer_count: h.document.layers.len(),
                memory: h.memory().unwrap_or_default(),
            },
            kind,
        )
    };
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
            "instructions": instructions,
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
                            format!("{instructions}\n\n{}", prompt::COMPACT_INSTRUCTIONS);
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
    {
        let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
        s.add_usage_krw(total_cost);
    }
    let _ = state.save_settings();
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

fn build_user_parts(
    h: &ProjectHandle,
    fonts: &Fonts,
    facts: Option<&prompt::OriginalFacts>,
    text: &str,
    ctx: &SendContext,
    summary: &[String],
    kind: TurnKind,
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
                msg.push_str(&format!(
                    "- {} ({}) · {:.0},{:.0} {:.0}×{:.0} mm\n",
                    l.id, l.name, l.x_mm, l.y_mm, l.w_mm, l.h_mm
                ));
            }
        }
        msg.push_str("</selected_layers>\n");
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

/// Rebuild only when there is nothing yet or the user asks to start over, so copies and templates edit normally.
pub fn turn_kind(layer_count: usize, has_original: bool, text: &str) -> TurnKind {
    let start_over = ["처음부터", "다시 만들어"].iter().any(|k| text.contains(k));
    if layer_count > 0 && !start_over {
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
    fn relayout_guard_flags_small_titles_wide_gaps_and_tall_boxes() {
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
        let rect = |rect: [f64; 4]| shape("board", 0.0, rect);
        let fonts = Fonts::new(Some(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"),
        ));
        assert!(
            relayout_guard_message(&h, &fonts).is_none(),
            "no text, nothing to judge"
        );
        h.document.layers = vec![
            text("청전동", 230.0, [600.0, 150.0, 940.0, 225.0]),
            text("주민역량강화", 230.0, [1540.0 + 600.0, 150.0, 1860.0, 225.0]),
            text("안내", 20.0, [100.0, 700.0, 900.0, 100.0]),
            rect([50.0, 570.0, 5900.0, 315.0]),
        ];
        let msg = relayout_guard_message(&h, &fonts).expect("problems");
        assert!(msg.contains("26%"), "{msg}");
        assert!(msg.contains("'청전동' and '주민역량강화'"), "{msg}");
        assert!(msg.contains("smallest text is 20 mm"), "{msg}");
        assert!(msg.contains("full-width box is 315 mm"), "{msg}");
        assert!(
            msg.contains("arrange_row") && msg.contains("with fit_box"),
            "{msg}"
        );
        h.document.layers = vec![
            text("청전동", 400.0, [300.0, 100.0, 1400.0, 420.0]),
            text("주민역량강화", 400.0, [1850.0, 100.0, 2600.0, 420.0]),
            text("안내", 40.0, [100.0, 700.0, 900.0, 100.0]),
            rect([50.0, 640.0, 5900.0, 220.0]),
        ];
        assert!(
            relayout_guard_message(&h, &fonts).is_none(),
            "a re-flowed layout passes"
        );
        h.document
            .layers
            .push(text("위 제목", 90.0, [2300.0, 100.0, 1300.0, 150.0]));
        let msg = relayout_guard_message(&h, &fonts).expect("band intrudes into the title");
        assert!(msg.contains("'위 제목' overlaps the title"), "{msg}");
        h.document.layers.pop();

        let ink = |h: &ProjectHandle, id: &str| tools::ink_mm(&fonts, h.document.layer(id).expect("layer")).0;
        let first = ink(&h, "청전동");
        let second = ink(&h, "주민역량강화");
        let even = second[0] - (first[0] + first[2]);
        let mut third = text("교육", 400.0, [0.0, 100.0, 1000.0, 420.0]);
        h.document.layers.push(third.clone());
        let lead = ink(&h, "교육")[0];
        third.x_mm = second[0] + second[2] + even + 0.2 * 400.0 - lead;
        *h.document.layers.last_mut().expect("third") = third.clone();
        let msg = relayout_guard_message(&h, &fonts).expect("uneven title gaps");
        assert!(
            msg.contains("title words are unevenly spaced") && msg.contains("call arrange_row"),
            "{msg}"
        );
        third.x_mm -= 0.2 * 400.0;
        *h.document.layers.last_mut().expect("third") = third;
        assert!(
            relayout_guard_message(&h, &fonts).is_none(),
            "evenly spaced title words pass"
        );
        h.document.layers.pop();

        h.document
            .layers
            .push(shape("알약", 35.0, [1000.0, 560.0, 1500.0, 70.0]));
        h.document
            .layers
            .push(text("알림", 40.0, [1100.0, 570.0, 300.0, 50.0]));
        let msg = relayout_guard_message(&h, &fonts).expect("pill longer than its text");
        assert!(
            msg.contains("'알약' is 1500 mm wide but its content is") && msg.contains("fit_box it"),
            "{msg}"
        );
        let label = ink(&h, "알림");
        if let Some(pill) = h.document.layers.iter_mut().find(|l| l.id == "알약") {
            pill.x_mm = label[0] - 20.0;
            pill.w_mm = label[2] + 40.0;
        }
        assert!(
            relayout_guard_message(&h, &fonts).is_none(),
            "a pill that hugs its text passes"
        );
        h.document.layers.truncate(4);

        h.document
            .layers
            .push(text("칸 0", 40.0, [1500.0, 700.0, 900.0, 100.0]));
        h.document
            .layers
            .push(text("칸 1", 40.0, [3000.0, 660.0, 300.0, 200.0]));
        if let Some(hoist_core::model::LayerKind::Text(t)) = h.document.layers.last_mut().map(|l| &mut l.kind)
        {
            t.text = "교육일정은 십일월 구일부터 십이월 십사일까지".into();
        }
        let msg = relayout_guard_message(&h, &fonts).expect("a column wraps into many lines");
        assert!(
            msg.contains("'칸 1' has") && msg.contains("lines: re-break the info columns to at most 2 lines"),
            "{msg}"
        );
        if let Some(l) = h.document.layers.last_mut() {
            l.w_mm = 1500.0;
        }
        assert!(
            relayout_guard_message(&h, &fonts).is_none(),
            "columns of at most 2 lines pass"
        );

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
            msg.contains("texts in Pretendard use different scale_x (1.00–1.20)"),
            "{msg}"
        );
        squeeze(&mut h, 1.03);
        assert!(
            relayout_guard_message(&h, &fonts).is_none(),
            "a small difference in scale_x passes"
        );
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
    fn layers_make_an_edit_turn_unless_the_user_starts_over() {
        assert_eq!(
            turn_kind(46, true, "전화번호를 043-123-4567로 바꿔줘"),
            TurnKind::Normal
        );
        assert_eq!(turn_kind(46, false, "제목을 키워줘"), TurnKind::Normal);
        assert_eq!(turn_kind(0, true, "만들어줘"), TurnKind::Reconstruct);
        assert_eq!(turn_kind(0, false, "가을 축제 현수막"), TurnKind::Scratch);
        assert_eq!(turn_kind(46, true, "처음부터 다시 해줘"), TurnKind::Reconstruct);
        assert_eq!(turn_kind(12, false, "다시 만들어 주세요"), TurnKind::Scratch);
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
