pub mod img;
pub mod prompt;
pub mod tools;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context};
use banner_core::check::export_check;
use banner_core::model::*;
use banner_core::ops::describe;
use banner_core::render::layer_bounds_mm;
use banner_core::store::ProjectHandle;
use banner_core::text::Fonts;
use banner_core::image as cimg;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::openai::{text_cost_krw, OpenAI, StreamEvent};
use crate::state::AppState;
use prompt::{original_facts, TurnKind};
use tools::{ToolCtx, TurnState};

pub struct AgentRun {
    pub running: AtomicBool,
    pub stop: AtomicBool,
    pub stop_notify: Notify,
    pub pending: Mutex<Option<(String, tokio::sync::oneshot::Sender<String>)>>,
}

impl AgentRun {
    pub fn new() -> Self {
        AgentRun { running: AtomicBool::new(false), stop: AtomicBool::new(false), stop_notify: Notify::new(), pending: Mutex::new(None) }
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

#[derive(Deserialize, Clone, Debug, Default)]
pub struct SendContext {
    #[serde(default)]
    pub layer_ids: Vec<String>,
    #[serde(default)]
    pub captures: Vec<Capture>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Capture {
    pub pane: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub w_mm: f64,
    pub h_mm: f64,
}

struct Emit {
    app: AppHandle,
    project_id: String,
    turn: String,
}

impl Emit {
    fn send(&self, mut payload: Value) {
        payload["project_id"] = json!(self.project_id);
        payload["turn"] = json!(self.turn);
        let _ = self.app.emit("agent-event", payload);
    }
}

fn items_path(h: &ProjectHandle) -> std::path::PathBuf {
    h.dir.join("agent_items.json")
}

fn agent_state_path(h: &ProjectHandle) -> std::path::PathBuf {
    h.dir.join("agent_state.json")
}

fn load_items(h: &ProjectHandle) -> Vec<Value> {
    std::fs::read(items_path(h)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_items(h: &ProjectHandle, items: &[Value]) {
    let _ = std::fs::write(items_path(h), serde_json::to_vec(items).unwrap_or_default());
}

fn load_agent_state(h: &ProjectHandle) -> Value {
    std::fs::read(agent_state_path(h)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({}))
}

fn save_agent_state(h: &ProjectHandle, v: &Value) {
    let _ = std::fs::write(agent_state_path(h), serde_json::to_vec(v).unwrap_or_default());
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn describe_entry(e: &OpEntry, doc: &Document) -> String {
    let name = |id: &str| doc.layer(id).map(|l| l.name.clone()).unwrap_or_else(|| id.to_string());
    match (&e.op, &e.inverse) {
        (Op::UpdateLayer { id, patch }, Op::UpdateLayer { patch: old, .. }) => {
            let paths: Vec<String> = banner_core::ops::patch_paths(patch).into_iter().filter(|p| p != "pinned").collect();
            let show = |v: Option<&Value>| match v {
                None | Some(Value::Null) => "없음".to_string(),
                Some(Value::Number(n)) => { let s = format!("{:.2}", n.as_f64().unwrap_or(0.0)); s.trim_end_matches('0').trim_end_matches('.').to_string() }
                Some(Value::String(s)) => s.chars().take(20).collect(),
                Some(Value::Array(a)) => format!("{}개", a.len()),
                Some(Value::Object(_)) => "{…}".into(),
                Some(Value::Bool(b)) => b.to_string(),
            };
            let mut parts: Vec<String> = paths.iter().take(3).map(|p| { let ptr = format!("/{}", p.replace('.', "/")); format!("{p} {} → {}", show(old.pointer(&ptr)), show(patch.pointer(&ptr))) }).collect();
            if paths.len() > 3 { parts.push(format!("외 {}개", paths.len() - 3)); }
            format!("{} · {}", name(id), parts.join(", "))
        }
        (Op::DeleteLayer { .. }, Op::AddLayer { layer, .. }) => format!("레이어 삭제 · {}", layer.name),
        (op, _) => describe(op, doc),
    }
}

pub fn edit_summary(h: &ProjectHandle, since_seq: u64) -> Vec<String> {
    let entries = h.entries_since(since_seq).unwrap_or_default();
    let mut lines: Vec<String> = Vec::new();
    for e in entries.iter().filter(|e| e.actor == Actor::User && !["undo:", "redo:", "restore:"].iter().any(|p| e.turn.as_deref().unwrap_or("").starts_with(p))) {
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

pub async fn run_turn(app: AppHandle, project_id: String, text: String, ctx_in: SendContext, kind: TurnKind) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    let key = settings.openai_key().ok_or_else(|| anyhow!("설정에서 OpenAI API 키를 먼저 넣어 주세요"))?;
    let openai = OpenAI::new(key)?;
    let handle = state.handle(&project_id)?;
    let fonts = state.fonts.clone();
    let run = {
        let mut agents = state.agents.lock().unwrap_or_else(|e| e.into_inner());
        agents.entry(project_id.clone()).or_insert_with(|| Arc::new(AgentRun::new())).clone()
    };
    if run.running.swap(true, Ordering::SeqCst) {
        anyhow::bail!("도우미가 이미 작업 중이에요");
    }
    run.stop.store(false, Ordering::SeqCst);
    let turn = uuid::Uuid::new_v4().simple().to_string();
    let emit = Emit { app: app.clone(), project_id: project_id.clone(), turn: turn.clone() };
    let result = run_turn_inner(&app, &state, &settings, openai, handle.clone(), fonts, run.clone(), &emit, text, ctx_in, kind).await;
    run.running.store(false, Ordering::SeqCst);
    *run.pending.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if let Err(e) = &result {
        emit.send(json!({"type": "error", "message": e.to_string()}));
        emit.send(json!({"type": "turn_end", "stopped": true}));
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
    let user_chat = json!({"id": user_item_id, "role": "user", "ts": now(), "text": text, "context": {"layer_ids": ctx_in.layer_ids, "captures": ctx_in.captures.iter().map(|c| json!({"pane": c.pane, "x_mm": c.x_mm, "y_mm": c.y_mm, "w_mm": c.w_mm, "h_mm": c.h_mm})).collect::<Vec<_>>()}, "edit_summary": summary_lines});
    let mut agent_chat = json!({"id": agent_item_id, "role": "agent", "ts": now(), "text": "", "actions": []});
    {
        let h = img::lock(&handle);
        h.append_chat(&user_chat)?;
        h.append_chat(&agent_chat)?;
    }
    emit.send(json!({"type": "turn_start"}));
    items.push(json!({"type": "message", "role": "user", "content": user_parts}));

    let turn_state = Arc::new(TurnState {
        asked: AtomicBool::new(false),
        must_ask_first: kind == TurnKind::ReplaceOriginal,
        edits: AtomicUsize::new(0),
        promoted: AtomicUsize::new(0),
        cost_krw: Mutex::new(0.0),
        memory_dirty: AtomicBool::new(false),
    });
    let question_sink = {
        let app = app.clone();
        let pid = emit.project_id.clone();
        let turn = turn.clone();
        let handle = handle.clone();
        let agent_item_id = agent_item_id.clone();
        Arc::new(move |qid: String, q: String, opts: Vec<String>| {
            let payload = json!({"type": "question", "project_id": pid, "turn": turn, "question": {"id": qid, "text": q, "options": opts}});
            let _ = app.emit("agent-event", payload.clone());
            let h = img::lock(&handle);
            if let Ok(chat) = h.chat() {
                if let Some(mut item) = chat.into_iter().find(|c| c["id"] == json!(agent_item_id)) {
                    item["question"] = payload["question"].clone();
                    let _ = h.update_chat(&agent_item_id, &item);
                }
            }
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
    let mut model = if heavy { settings.heavy_model.clone() } else { settings.agent_model.clone() };
    let effort = if heavy { "high" } else { "medium" };
    let instructions = {
        let h = img::lock(&handle);
        prompt::instructions(&h.project, &prompt::PromptFacts { original: facts.as_ref().map(|f| prompt::OriginalFacts { ..*f }), layer_count: h.document.layers.len(), memory: h.memory().unwrap_or_default() }, kind)
    };
    let tools = tools::definitions();
    let mut full_text = String::new();
    let mut actions: Vec<Value> = Vec::new();
    let mut total_cost = 0.0;
    let mut guard_fails = 0;
    let mut stopped = false;
    let mut rounds = 0;
    let mut image_outputs_ok = true;

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
        if serde_json::to_string(&items).map(|s| s.len()).unwrap_or(0) > 400_000 {
            match openai.compact(&settings.cheap_model, &items, &instructions).await {
                Ok(compacted) if !compacted.is_empty() => items = compacted,
                _ => {}
            }
        }
        let body = json!({
            "model": model,
            "instructions": instructions,
            "input": items,
            "tools": tools,
            "tool_choice": "auto",
            "parallel_tool_calls": false,
            "reasoning": {"effort": effort},
            "store": false,
            "include": ["reasoning.encrypted_content"],
            "max_output_tokens": 12000
        });
        let emit_text = emit.clone_for_text();
        let mut round_text = String::new();
        let out = match openai
            .responses_stream(body, |ev| {
                if let StreamEvent::TextDelta(d) = ev {
                    round_text.push_str(&d);
                    emit_text.send(json!({"type": "text_delta", "delta": d}));
                }
            })
            .await
        {
            Ok(o) => o,
            Err(e) => {
                if image_outputs_ok && e.to_string().contains("function_call_output") {
                    image_outputs_ok = false;
                    items = strip_image_outputs(items);
                    continue;
                }
                return Err(e);
            }
        };
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
                    let aid = push_action(&mut actions, "가드 · 원본과 비교", "failed");
                    emit.send(json!({"type": "action", "action": actions[aid]}));
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
            let args: Value = serde_json::from_str(call["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}));
            let label = tools::label_for(&name, &args);
            let aid = push_action(&mut actions, &label, "running");
            emit.send(json!({"type": "action", "action": actions[aid]}));
            let result = tools::execute(&tool_ctx, &name, args).await;
            let output = match result {
                Ok(r) => {
                    actions[aid]["status"] = json!("done");
                    if tools_edit(&name) {
                        let rev = img::lock(&handle).document.rev;
                        emit.send(json!({"type": "document_changed", "rev": rev}));
                    }
                    tool_output(&call_id, r, image_outputs_ok)
                }
                Err(e) => {
                    actions[aid]["status"] = json!("failed");
                    actions[aid]["label"] = json!(format!("{label} · {}", short(&e.to_string())));
                    json!({"type": "function_call_output", "call_id": call_id, "output": json!({"error": e.to_string()}).to_string()})
                }
            };
            emit.send(json!({"type": "action", "action": actions[aid]}));
            items.push(output);
            agent_chat["actions"] = json!(actions);
            let h = img::lock(&handle);
            let _ = h.update_chat(&agent_item_id, &agent_chat);
        }
    }
    total_cost += *turn_state.cost_krw.lock().unwrap_or_else(|e| e.into_inner());

    let report = if turn_state.edits.load(Ordering::SeqCst) > 0 {
        let h = img::lock(&handle);
        match facts.as_ref() {
            Some(f) => {
                let fc = img::compare_full(&h, &fonts, f, 1200)?;
                let warnings = export_check(&h.document, h.project.target_dpi);
                let min_dpi = warnings.iter().map(|w| w.effective_dpi).fold(f64::INFINITY, f64::min);
                let min_dpi = if min_dpi.is_finite() { min_dpi } else { h.project.target_dpi };
                let promoted = turn_state.promoted.load(Ordering::SeqCst);
                let summary = format!("유사도 {:.0}% · 겹침 {:.0}% · 실물 {}dpi{} · 방식 바꿈 {}", fc.result.similarity * 100.0, fc.result.overlap * 100.0, min_dpi.round(), if warnings.is_empty() { " 충족" } else { " 부족" }, promoted);
                Some(json!({"original_png": img::data_url(&fc.original_png), "work_png": img::data_url(&fc.work_png), "overlap": fc.result.overlap, "similarity": fc.result.similarity, "min_dpi": min_dpi, "promoted": promoted, "summary": summary}))
            }
            None => {
                let png = img::thumbnail_png(&h, &fonts)?;
                Some(json!({"original_png": "", "work_png": img::data_url(&png), "overlap": 0.0, "similarity": 0.0, "min_dpi": h.project.target_dpi, "promoted": 0, "summary": "시안"}))
            }
        }
    } else {
        None
    };
    if let Some(r) = &report {
        emit.send(json!({"type": "report", "report": r}));
        agent_chat["report"] = r.clone();
    }
    if stopped && full_text.is_empty() {
        full_text = "작업을 중단했어요.".into();
    }
    agent_chat["text"] = json!(full_text);
    agent_chat["actions"] = json!(actions);
    emit.send(json!({"type": "text", "text": full_text}));
    {
        let mut h = img::lock(&handle);
        let _ = h.update_chat(&agent_item_id, &agent_chat);
        save_items(&h, &items);
        if turn_state.edits.load(Ordering::SeqCst) > 0 {
            let thumb = img::thumbnail_png(&h, &fonts).ok();
            let summary = full_text.lines().find(|l| !l.trim().is_empty()).unwrap_or("도우미 작업").chars().take(40).collect::<String>();
            let _ = h.snapshot(Actor::Agent, &format!("{summary} · 도우미"), Some(turn.clone()), thumb.as_deref());
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
    emit.send(json!({"type": "turn_end", "stopped": stopped}));
    Ok(())
}

impl Emit {
    fn clone_for_text(&self) -> Emit {
        Emit { app: self.app.clone(), project_id: self.project_id.clone(), turn: self.turn.clone() }
    }
}

fn tools_edit(name: &str) -> bool {
    matches!(name, "add_layer" | "update_layer" | "delete_layer" | "move_layer" | "trace_region")
}

fn short(s: &str) -> String {
    s.chars().take(60).collect()
}

fn push_action(actions: &mut Vec<Value>, label: &str, status: &str) -> usize {
    actions.push(json!({"id": uuid::Uuid::new_v4().simple().to_string(), "label": label, "status": status}));
    actions.len() - 1
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
                let text = it["output"].as_array().and_then(|a| a.iter().find(|p| p["type"] == "input_text")).and_then(|p| p["text"].as_str()).unwrap_or("").to_string();
                it["output"] = json!(text);
            }
            it
        })
        .collect()
}

fn build_user_parts(h: &ProjectHandle, fonts: &Fonts, facts: Option<&prompt::OriginalFacts>, text: &str, ctx: &SendContext, summary: &[String], kind: TurnKind) -> anyhow::Result<Vec<Value>> {
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
                msg.push_str(&format!("- {} ({}) · {:.0},{:.0} {:.0}×{:.0} mm\n", l.id, l.name, l.x_mm, l.y_mm, l.w_mm, l.h_mm));
            }
        }
    }
    for c in &ctx.captures {
        msg.push_str(&format!("\n[캡처 · {} · x {:.0} y {:.0} w {:.0} h {:.0} mm] 아래 이미지 참고\n", if c.pane == "original" { "원본" } else { "작업본" }, c.x_mm, c.y_mm, c.w_mm, c.h_mm));
    }
    if kind == TurnKind::ReplaceOriginal {
        msg.push_str("\n");
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
        let im = if c.pane == "original" {
            match facts {
                Some(f) => {
                    let orig = img::original_image(h, f.version)?;
                    let px = |mm: f64, off: f64| ((mm - off) / f.mm_per_px).max(0.0) as u32;
                    cimg::crop(&orig, [px(rect[0], f.offset_x_mm), px(rect[1], f.offset_y_mm), (rect[2] / f.mm_per_px).max(1.0) as u32, (rect[3] / f.mm_per_px).max(1.0) as u32])
                }
                None => continue,
            }
        } else {
            img::work_region(h, fonts, rect, 1000)?
        };
        parts.push(json!({"type": "input_image", "image_url": img::data_url(&cimg::to_png_bytes(&cimg::resize_max(&im, 1000))?), "detail": "high"}));
    }
    if kind == TurnKind::Reconstruct || kind == TurnKind::ReplaceOriginal {
        if let Some(f) = facts {
            let orig = img::original_image(h, f.version)?;
            parts.push(json!({"type": "input_text", "text": format!("[원본 v{} 전체 이미지]", f.version)}));
            parts.push(json!({"type": "input_image", "image_url": img::data_url(&cimg::to_png_bytes(&cimg::resize_max(&orig, 1536))?), "detail": "high"}));
        }
    }
    Ok(parts)
}

pub fn spawn_turn(app: AppHandle, project_id: String, text: String, ctx: SendContext, kind: TurnKind) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_turn(app, project_id, text, ctx, kind).await {
            eprintln!("agent turn failed: {e:#}");
        }
    });
}

pub fn is_first_turn(h: &ProjectHandle) -> bool {
    h.chat().map(|c| c.is_empty()).unwrap_or(true)
}

pub fn load_context(h: &ProjectHandle) -> anyhow::Result<Value> {
    Ok(json!({"chat": h.chat().context("대화 기록")?, "memory": h.memory().unwrap_or_default()}))
}

pub fn facts_for(h: &ProjectHandle) -> Option<prompt::OriginalFacts> {
    original_facts(&h.project)
}
