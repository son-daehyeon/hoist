pub mod img;
pub mod prompt;
pub mod tools;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use hoist_core::check::export_check;
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

use crate::openai::{ContextOverflow, OpenAI, StreamDelta, text_cost_krw};
use crate::state::AppState;
use prompt::{OriginalFacts, TurnKind, original_facts};
use tools::{ToolCtx, TurnState};

pub struct AgentRun {
    pub running: AtomicBool,
    pub stop: AtomicBool,
    pub stop_notify: Notify,
    pub pending: Mutex<Option<(String, tokio::sync::oneshot::Sender<String>)>>,
}

impl AgentRun {
    pub fn new() -> Self {
        AgentRun {
            running: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            stop_notify: Notify::new(),
            pending: Mutex::new(None),
        }
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.stop_notify.notify_waiters();
    }

    pub fn answer(&self, question_id: &str, answer: &str) -> bool {
        let mut p = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        match p.take() {
            Some((qid, tx)) if qid == question_id => tx.send(answer.to_string()).is_ok(),
            Some(other) => {
                *p = Some(other);
                false
            }
            None => false,
        }
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
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct ActionItem {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub status: ActionStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct Report {
    pub original_png: String,
    pub work_png: String,
    pub overlap: f64,
    pub similarity: f64,
    pub min_dpi: f64,
    pub promoted: usize,
    pub summary: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
pub struct Question {
    pub id: String,
    pub text: String,
    pub options: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatItem {
    User(UserChat),
    Agent(AgentChat),
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
    ThinkingStart { round: usize },
    ThinkingDelta { delta: String },
    Compacted { reason: String },
    TextDelta { delta: String },
    Text { text: String },
    Action { action: ActionItem },
    DocumentChanged { rev: u64 },
    Report { report: Report },
    Question { question: Question },
    TurnEnd { stopped: bool },
    Error { message: String },
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
    *run.pending.lock().unwrap_or_else(|e| e.into_inner()) = None;
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
    let agent_item_id = uuid::Uuid::new_v4().simple().to_string();
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
    let agent_chat = ChatItem::Agent(AgentChat {
        id: agent_item_id.clone(),
        ts: now(),
        text: String::new(),
        actions: Vec::new(),
        report: None,
        question: None,
        answer: None,
    });
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
        edits: AtomicUsize::new(0),
        promoted: AtomicUsize::new(0),
        cost_krw: Mutex::new(0.0),
        memory_dirty: AtomicBool::new(false),
        round: AtomicUsize::new(0),
        memo: tools::ReadMemo::default(),
    });
    let question_sink = {
        let emit = emit.clone();
        let handle = handle.clone();
        let agent_item_id = agent_item_id.clone();
        Arc::new(move |qid: String, q: String, opts: Vec<String>| {
            let question = Question {
                id: qid,
                text: q,
                options: opts,
            };
            emit.send(AgentEventKind::Question {
                question: question.clone(),
            });
            let h = img::lock(&handle);
            update_agent_chat(&h, |c| c.id == agent_item_id, |c| c.question = Some(question));
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
    };
    let heavy = matches!(kind, TurnKind::Reconstruct | TurnKind::Scratch);
    let mut model = if heavy {
        settings.heavy_model.clone()
    } else {
        settings.agent_model.clone()
    };
    let effort = if heavy { "high" } else { "medium" };
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
    let tools = tools::definitions();
    let mut full_text = String::new();
    let mut actions: Vec<ActionItem> = Vec::new();
    let mut total_cost = 0.0;
    let mut guard_fails = 0;
    let mut stopped = false;
    let mut rounds: usize = 0;
    let mut image_outputs_ok = true;
    let mut thinking_ok = true;
    let mut thinking_detailed = true;
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut last_input_tokens = 0u64;
    let mut overflow_stage = 0;

    'outer: loop {
        if rounds > 80 {
            full_text.push_str("\n\n(도구 호출 한도에 닿아 여기서 멈췄어요.)");
            break;
        }
        if run.stop.load(Ordering::SeqCst) {
            stopped = true;
            break;
        }
        rounds += 1;
        turn_state.round.store(rounds, Ordering::SeqCst);
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
            "parallel_tool_calls": false,
            "reasoning": if thinking_ok {
                json!({"effort": effort, "summary": if thinking_detailed { "detailed" } else { "auto" }})
            } else {
                json!({"effort": effort})
            },
            "prompt_cache_key": emit.project_id,
            "store": false,
            "include": ["reasoning.encrypted_content"],
            "max_output_tokens": 12000
        });
        emit.send(AgentEventKind::ThinkingStart { round: rounds });
        let mut round_text = String::new();
        let out = match openai
            .responses_stream(body, |delta| match delta {
                StreamDelta::Text(d) => {
                    round_text.push_str(d);
                    emit.send(AgentEventKind::TextDelta { delta: d.to_string() });
                }
                StreamDelta::Thinking(d) => emit.send(AgentEventKind::ThinkingDelta { delta: d.to_string() }),
            })
            .await
        {
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
                if thinking_ok
                    && (message.contains("reasoning summar") || message.contains("reasoning.summary"))
                {
                    if thinking_detailed {
                        thinking_detailed = false;
                    } else {
                        thinking_ok = false;
                    }
                    continue;
                }
                if image_outputs_ok && message.contains("function_call_output") {
                    image_outputs_ok = false;
                    items = strip_image_outputs(items);
                    continue;
                }
                return Err(e);
            }
        };
        last_input_tokens = out.usage.input_tokens;
        total_cost += text_cost_krw(&model, &out.usage);
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
            if turn_state.edits.load(Ordering::SeqCst) > 0 && facts.is_some() && guard_fails < 2 {
                let (sim, png) = {
                    let h = img::lock(&handle);
                    let f = facts.as_ref().ok_or_else(|| anyhow!("원본 없음"))?;
                    let fc = img::compare_full(&h, &fonts, f, 1200)?;
                    (fc.result.similarity, fc.side_by_side_png)
                };
                if sim < 0.85 {
                    guard_fails += 1;
                    if guard_fails == 2 {
                        model = settings.heavy_model.clone();
                    }
                    let aid = push_action(&mut actions, "가드 · 원본과 비교", None, ActionStatus::Failed);
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
            break;
        }
        for call in calls {
            if run.stop.load(Ordering::SeqCst) {
                stopped = true;
                break 'outer;
            }
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
            let aid = push_action(&mut actions, &label, detail.clone(), ActionStatus::Running);
            emit.send(AgentEventKind::Action {
                action: actions[aid].clone(),
            });
            let result = tools::execute(&tool_ctx, &name, args).await;
            let output = match result {
                Ok(r) => {
                    actions[aid].status = ActionStatus::Done;
                    if r.cached {
                        actions[aid].detail = Some(match detail {
                            Some(d) => format!("{d} · 이미 본 결과"),
                            None => "이미 본 결과".into(),
                        });
                    }
                    if tools_edit(&name) {
                        let rev = img::lock(&handle).document.rev;
                        emit.send(AgentEventKind::DocumentChanged { rev });
                    }
                    tool_output(&call_id, r, image_outputs_ok)
                }
                Err(e) => {
                    actions[aid].status = ActionStatus::Failed;
                    actions[aid].label = format!("{label} · {}", short(&e.to_string()));
                    json!({"type": "function_call_output", "call_id": call_id, "output": json!({"error": e.to_string()}).to_string()})
                }
            };
            emit.send(AgentEventKind::Action {
                action: actions[aid].clone(),
            });
            items.push(output);
            let h = img::lock(&handle);
            update_agent_chat(&h, |c| c.id == agent_item_id, |c| c.actions = actions.clone());
        }
    }
    total_cost += *turn_state.cost_krw.lock().unwrap_or_else(|e| e.into_inner());

    let report = if turn_state.edits.load(Ordering::SeqCst) > 0 {
        let h = img::lock(&handle);
        match facts.as_ref() {
            Some(f) => {
                let fc = img::compare_full(&h, &fonts, f, 1200)?;
                let warnings = export_check(&h.document, h.project.target_dpi);
                let min_dpi = warnings
                    .iter()
                    .map(|w| w.effective_dpi)
                    .fold(f64::INFINITY, f64::min);
                let min_dpi = if min_dpi.is_finite() {
                    min_dpi
                } else {
                    h.project.target_dpi
                };
                let promoted = turn_state.promoted.load(Ordering::SeqCst);
                let summary = format!(
                    "유사도 {:.0}% · 겹침 {:.0}% · 실물 {}dpi{} · 방식 바꿈 {}",
                    fc.result.similarity * 100.0,
                    fc.result.overlap * 100.0,
                    min_dpi.round(),
                    if warnings.is_empty() { " 충족" } else { " 부족" },
                    promoted
                );
                Some(Report {
                    original_png: img::data_url(&fc.original_png),
                    work_png: img::data_url(&fc.work_png),
                    overlap: fc.result.overlap,
                    similarity: fc.result.similarity,
                    min_dpi,
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
    Ok(())
}

fn tools_edit(name: &str) -> bool {
    matches!(
        name,
        "add_layer" | "update_layer" | "delete_layer" | "move_layer" | "trace_region"
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
) -> usize {
    actions.push(ActionItem {
        id: uuid::Uuid::new_v4().simple().to_string(),
        label: label.to_string(),
        detail,
        status,
    });
    actions.len() - 1
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
                .map(|r| format!("라운드 {r}에서 봄"))
                .unwrap_or_else(|| "이전 턴에서 봄".into());
            format!("(이미지 생략: {what}, {when} — 다시 필요하면 같은 도구를 다른 영역/배율로 호출)")
        }
        None => "(이미지 생략: 메시지에 붙어 있던 이미지, 이미 봄 — 다시 필요하면 view_original이나 view_work로 호출)".into(),
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
        msg.push_str("\n\n[사용자가 직접 수정함 — 이 속성들은 고정됨]\n");
        for l in summary {
            msg.push_str("- ");
            msg.push_str(l);
            msg.push('\n');
        }
    }
    if !ctx.layer_ids.is_empty() {
        msg.push_str("\n[선택한 레이어]\n");
        for id in &ctx.layer_ids {
            if let Some(l) = h.document.layer(id) {
                msg.push_str(&format!(
                    "- {} ({}) · {:.0},{:.0} {:.0}×{:.0} mm\n",
                    l.id, l.name, l.x_mm, l.y_mm, l.w_mm, l.h_mm
                ));
            }
        }
    }
    for c in &ctx.captures {
        msg.push_str(&format!(
            "\n[캡처 · {} · x {:.0} y {:.0} w {:.0} h {:.0} mm] 아래 이미지 참고\n",
            if c.pane == Pane::Original {
                "원본"
            } else {
                "작업본"
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
            if let Ok(im) = img::work_region(h, fonts, rect, 800) {
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
            img::work_region(h, fonts, rect, 1000)?
        };
        parts.push(json!({"type": "input_image", "image_url": img::data_url(&cimg::to_png_bytes(&cimg::resize_max(&im, 1000))?), "detail": "high"}));
    }
    if (kind == TurnKind::Reconstruct || kind == TurnKind::ReplaceOriginal)
        && let Some(f) = facts
    {
        let orig = img::original_image(h, f.version)?;
        parts.push(json!({"type": "input_text", "text": format!("[원본 v{} 전체 이미지]", f.version)}));
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

pub fn is_first_turn(h: &ProjectHandle) -> bool {
    h.chat().map(|c| c.is_empty()).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            json!({"id": "a2", "role": "agent", "ts": "2026-11-05T00:00:00Z", "text": "done", "actions": [{"id": "x", "label": "l", "status": "done"}], "question": {"id": "q", "text": "?", "options": ["a", "b"]}, "answer": "a", "report": {"original_png": "", "work_png": "", "overlap": 0.5, "similarity": 0.9, "min_dpi": 100.0, "promoted": 1, "summary": "s"}}),
        ];
        for v in stored {
            let item: ChatItem = serde_json::from_value(v.clone()).expect("parse stored chat item");
            assert_eq!(serde_json::to_value(item).expect("serialize chat item"), v);
        }
    }
}
