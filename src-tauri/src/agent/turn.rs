use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use futures_util::FutureExt;
use hoist_core::model::{Actor, FitMode};
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::calls::run_calls;
use super::chat::{open_turn, update_agent_chat};
use super::classify::{self, TurnClass};
use super::context::{
    DEMOTE_AT_INPUT_TOKENS, Input, KEEP_IMAGES, KEEP_IMAGES_ON_OVERFLOW, load_agent_state, save_agent_state,
    strip_image_outputs, tool_error, tool_output,
};
use super::edit_summary::edit_summary;
use super::events::{
    ActionItem, ActionStatus, AgentEvent, AgentEventKind, Emit, Question, Report, SendContext,
};
use super::guards::{COMPARE_MAX_PX, Guards};
use super::prompt::{self, OriginalFacts, TurnKind, original_facts};
use super::tools::{self, ReadMemo, ToolCtx, TurnState};
use super::user_message::{attachments, message_text};
use super::{AgentRun, img, layer_args};
use crate::openai::{ContextOverflow, OpenAI, decision_cost_krw, text_cost_krw};
use crate::popups::NoticeKind;
use crate::settings::Settings;
use crate::state::{AppState, lock};

const MAX_ROUNDS: usize = 200;

pub fn spawn_turn(
    app: AppHandle,
    project_id: String,
    text: String,
    sent_context: SendContext,
    kind: TurnKind,
    on_event: Channel<AgentEvent>,
) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_turn(app, project_id, text, sent_context, kind, on_event).await {
            log::error!("agent turn failed: {e:#}");
        }
    });
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

async fn run_turn(
    app: AppHandle,
    project_id: String,
    text: String,
    sent_context: SendContext,
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
        let mut agents = lock(&state.agents);
        agents
            .entry(project_id.clone())
            .or_insert_with(|| Arc::new(AgentRun::new()))
            .clone()
    };
    if run.running.swap(true, Ordering::SeqCst) {
        anyhow::bail!("에이전트가 이미 작업 중이에요");
    }
    run.stop.store(false, Ordering::SeqCst);
    let emit = Emit {
        channel: on_event,
        project_id,
        turn: uuid::Uuid::new_v4().simple().to_string(),
    };
    let turn = Turn {
        app: &app,
        state: &state,
        settings: &settings,
        openai,
        handle: handle.clone(),
        fonts,
        run: run.clone(),
        emit: &emit,
    };
    let result = caught(turn.run(text, sent_context, kind)).await;
    run.running.store(false, Ordering::SeqCst);
    lock(&run.pending).clear();
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
        emit.send(AgentEventKind::TurnEnd { stopped: false });
    }
    result
}

async fn caught<T>(turn: impl Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
    std::panic::AssertUnwindSafe(turn)
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(anyhow!("에이전트가 예기치 않게 멈췄어요. 다시 시도해 주세요")))
}

/// A turn runs for minutes and the window may be in the tray: say when it ends or needs an answer.
fn notify(app: &AppHandle, handle: &Arc<Mutex<ProjectHandle>>, kind: NoticeKind) {
    let project = lock(handle).project.clone();
    crate::popups::notify(
        app,
        crate::popups::Notice {
            project_id: project.id,
            project_name: project.name,
            kind,
        },
    );
}

struct Turn<'a> {
    app: &'a AppHandle,
    state: &'a AppState,
    settings: &'a Settings,
    openai: OpenAI,
    handle: Arc<Mutex<ProjectHandle>>,
    fonts: Arc<Fonts>,
    run: Arc<AgentRun>,
    emit: &'a Emit,
}

struct Classified {
    class: TurnClass,
    kind: TurnKind,
    krw: f64,
    label: Option<String>,
}

impl Turn<'_> {
    async fn run(&self, text: String, sent_context: SendContext, kind: TurnKind) -> anyhow::Result<()> {
        let (handle, fonts, emit) = (&self.handle, &self.fonts, self.emit);
        // The agent chat item shares the turn id: the UI's live item and the stored item are one entry.
        let agent_item_id = emit.turn.clone();
        let Classified {
            class,
            kind,
            krw,
            label,
        } = self.classify(&text, &sent_context.layer_ids, kind).await;
        let (facts, mut input, summary, user_parts) = {
            let h = lock(handle);
            let facts = original_facts(&h.project);
            let input = Input::load(&h, class == TurnClass::Simple);
            let last_seq = load_agent_state(&h)["last_seq"].as_u64().unwrap_or(0);
            let summary = edit_summary(&h, last_seq);
            let message = message_text(&h, &text, &sent_context, &summary, kind, class);
            let mut parts = vec![json!({"type": "input_text", "text": message})];
            parts.extend(attachments(&h, fonts, facts.as_ref(), &sent_context, kind)?);
            (facts, input, summary, parts)
        };
        open_turn(&lock(handle), &agent_item_id, text, sent_context, summary)?;
        emit.send(AgentEventKind::TurnStart);
        input
            .items
            .push(json!({"type": "message", "role": "user", "content": user_parts}));

        let turn_state = Arc::new(TurnState {
            must_ask_first: kind == TurnKind::ReplaceOriginal,
            fit_pending: AtomicBool::new(prompt::needs_fit_answer(&lock(handle).project, facts.as_ref())),
            plan_pending: AtomicBool::new(matches!(kind, TurnKind::Reconstruct | TurnKind::Scratch)),
            ..Default::default()
        });
        let tool_ctx = self.tool_ctx(turn_state.clone(), &agent_item_id);
        let mut progress = Progress {
            actions: ActionLog {
                items: Vec::new(),
                emit,
                handle,
                chat_id: &agent_item_id,
            },
            billing: Billing {
                total: 0.0,
                images_billed: 0.0,
                turn_state: &turn_state,
                handle,
                emit,
            },
            text: String::new(),
            report_at: None,
            stopped: false,
            thinking: Duration::ZERO,
        };
        if let Some(label) = label {
            progress.actions.announce(&label, None, ActionStatus::Done, 0);
            progress.billing.add(krw);
        }
        let result = self
            .rounds(&tool_ctx, facts.as_ref(), kind, class, &mut input, &mut progress)
            .await;
        self.finish(result, &tool_ctx, facts.as_ref(), input, progress)
    }

    async fn classify(&self, text: &str, layer_ids: &[String], mut kind: TurnKind) -> Classified {
        let (classify_input, layer_count, has_original) = {
            let h = lock(&self.handle);
            (
                classify::input(&h, text, layer_ids),
                h.document.layers.len(),
                !h.project.originals.is_empty(),
            )
        };
        let mut class = TurnClass::from_keywords(text);
        let mut krw = 0.0;
        let mut label = None;
        if kind != TurnKind::ReplaceOriginal && layer_count > 0 {
            let asked_at = Instant::now();
            match classify::classify(&self.openai, &self.settings.cheap_model, &classify_input).await {
                Ok(v) => {
                    krw = decision_cost_krw(&v.usage);
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
                    label = Some(format!("분류 · {}", class.label()));
                }
                Err(e) => {
                    log::warn!("turn classification skipped: {e}");
                    label = Some(format!(
                        "분류 · 실패해 기존 경로 · {}",
                        error_label(&e.to_string())
                    ));
                }
            }
            kind = turn_kind(layer_count, has_original, class);
        }
        Classified {
            class,
            kind,
            krw,
            label,
        }
    }

    fn tool_ctx(&self, turn_state: Arc<TurnState>, agent_item_id: &str) -> ToolCtx {
        let on_question = {
            let emit = self.emit.clone();
            let handle = self.handle.clone();
            let agent_item_id = agent_item_id.to_string();
            let app = self.app.clone();
            Arc::new(move |question: Question| {
                emit.send(AgentEventKind::Question {
                    question: question.clone(),
                });
                notify(&app, &handle, NoticeKind::Question);
                let h = lock(&handle);
                update_agent_chat(&h, |c| c.id == agent_item_id, |c| c.record_question(question));
            })
        };
        ToolCtx {
            handle: self.handle.clone(),
            fonts: self.fonts.clone(),
            openai: self.openai.clone(),
            image_model: self.settings.image_model.clone(),
            turn: self.emit.turn.clone(),
            facts: original_facts(&lock(&self.handle).project),
            run: self.run.clone(),
            state: turn_state,
            on_question,
            models_dir: self.state.models_dir.clone(),
        }
    }

    async fn rounds(
        &self,
        tool_ctx: &ToolCtx,
        facts: Option<&OriginalFacts>,
        kind: TurnKind,
        class: TurnClass,
        input: &mut Input,
        p: &mut Progress<'_>,
    ) -> anyhow::Result<()> {
        let (settings, handle, emit) = (self.settings, &self.handle, self.emit);
        let turn_state = &tool_ctx.state;
        let needs_plan = matches!(kind, TurnKind::Reconstruct | TurnKind::Scratch);
        // A simple edit stays on the agent model at low effort: it already drives the strict layer schemas
        // reliably, and with the layer records in the first request a wrong edit costs more than the tokens.
        let (mut model, mut effort) = if needs_plan {
            (settings.heavy_model.clone(), "high")
        } else if class == TurnClass::Simple {
            (settings.agent_model.clone(), "low")
        } else {
            (settings.agent_model.clone(), "medium")
        };
        let mut planned = false;
        let mut instructions = TurnPrompt::build(&lock(handle), facts, kind);
        let mut guards = Guards::default();
        let mut rounds: usize = 0;
        loop {
            if rounds >= MAX_ROUNDS {
                p.text.push_str("\n\n(도구 호출 한도에 닿아 여기서 멈췄어요.)");
                break;
            }
            if self.run.stop.load(Ordering::SeqCst) {
                p.stopped = true;
                break;
            }
            rounds += 1;
            turn_state.round.store(rounds, Ordering::SeqCst);
            instructions.refresh(&lock(handle), facts, kind);
            if needs_plan && !planned && lock(&turn_state.plan).is_some() {
                planned = true;
                model = settings.agent_model.clone();
                effort = "medium";
            }
            let max_output_tokens = if effort == "high" { 32_000 } else { 12_000 };
            log::debug!("round {rounds} model={model} effort={effort} max_output_tokens={max_output_tokens}");
            if input.last_input_tokens > DEMOTE_AT_INPUT_TOKENS {
                input.demote(KEEP_IMAGES, handle, facts, &turn_state.memo);
            }
            let body = json!({
                "model": model,
                "instructions": instructions.text,
                "input": input.items,
                "tools": input.tools,
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
            let asked_at = Instant::now();
            let response = self
                .openai
                .responses_stream(body, |d| {
                    round_text.push_str(d);
                    emit.send(AgentEventKind::TextDelta { delta: d.to_string() });
                })
                .await;
            p.thinking += asked_at.elapsed();
            let out = match response {
                Ok(o) => {
                    input.overflow_stage = 0;
                    o
                }
                Err(e) => {
                    self.recover(e, input, facts, &turn_state.memo, &model, &instructions.text)
                        .await?;
                    continue;
                }
            };
            input.last_input_tokens = out.usage.input_tokens;
            let round_text_krw = text_cost_krw(&model, &out.usage);
            let before_round = p.text.len();
            let round_text = korean_only(&round_text);
            if !round_text.is_empty() {
                if !p.text.is_empty() {
                    p.text.push_str("\n\n");
                }
                p.text.push_str(&round_text);
            }
            let mut calls: Vec<Value> = Vec::new();
            for item in &out.output {
                input.items.push(item.clone());
                if item["type"] == "function_call" {
                    calls.push(item.clone());
                }
            }
            if calls.is_empty() {
                if !round_text.is_empty() {
                    p.replace_report(before_round);
                }
                p.billing.add(round_text_krw);
                if turn_state.wording_only() {
                    break;
                }
                let step = guards.next(tool_ctx, facts, class, &settings.cheap_model).await?;
                if let Some(krw) = step.review_krw {
                    p.billing.add(krw);
                }
                if let Some(label) = &step.label {
                    p.actions.announce(label, None, ActionStatus::Done, rounds);
                }
                if step.escalate {
                    model = settings.heavy_model.clone();
                    effort = "high";
                }
                match step.send_back {
                    Some(message) => {
                        input.items.push(message);
                        continue;
                    }
                    None => break,
                }
            }
            let interrupted = self
                .tool_round(tool_ctx, calls, rounds, &mut p.actions, input)
                .await;
            p.billing.add(round_text_krw);
            if interrupted {
                p.stopped = true;
                break;
            }
        }
        Ok(())
    }

    async fn recover(
        &self,
        e: anyhow::Error,
        input: &mut Input,
        facts: Option<&OriginalFacts>,
        memo: &ReadMemo,
        model: &str,
        instructions: &str,
    ) -> anyhow::Result<()> {
        if e.downcast_ref::<ContextOverflow>().is_some() {
            if input.overflow_stage == 0
                && input.demote(KEEP_IMAGES_ON_OVERFLOW, &self.handle, facts, memo) > 0
            {
                input.overflow_stage = 1;
                return Ok(());
            }
            if input.overflow_stage < 2 {
                let compact_instructions = format!("{}\n\n{}", instructions, prompt::COMPACT_INSTRUCTIONS);
                let compacted = self
                    .openai
                    .compact(model, &input.items, &compact_instructions)
                    .await?;
                if !compacted.is_empty() {
                    input.compacted(compacted);
                    input.overflow_stage = 2;
                    self.emit.send(AgentEventKind::Compacted {
                        reason: "컨텍스트 한도 초과".into(),
                    });
                    return Ok(());
                }
            }
            return Err(e);
        }
        let message = e.to_string();
        if input.strict_tools && (message.contains("Invalid schema") || message.contains("param: tools[")) {
            input.strict_tools = false;
            layer_args::strip_strict(&mut input.tools);
            return Ok(());
        }
        if input.image_outputs_ok && message.contains("function_call_output") {
            input.image_outputs_ok = false;
            input.items = strip_image_outputs(std::mem::take(&mut input.items));
            return Ok(());
        }
        Err(e)
    }

    async fn tool_round(
        &self,
        tool_ctx: &ToolCtx,
        calls: Vec<Value>,
        round: usize,
        actions: &mut ActionLog<'_>,
        input: &mut Input,
    ) -> bool {
        let (handle, run, emit) = (&self.handle, &self.run, self.emit);
        let mut pending: Vec<PendingCall> = Vec::new();
        let mut jobs: Vec<(String, Value)> = Vec::new();
        for call in calls {
            let name = call["name"].as_str().unwrap_or("").to_string();
            let call_id = call["call_id"].as_str().unwrap_or("").to_string();
            let args: Value =
                serde_json::from_str(call["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}));
            let label = tools::label_for(&name, &args);
            let detail = {
                let h = lock(handle);
                tools::detail_for(&name, &args, &h.document, tool_ctx.facts.as_ref())
            };
            input.seen.insert(call_id.clone(), round);
            let action_index = actions.announce(&label, detail.clone(), ActionStatus::Running, round);
            pending.push(PendingCall {
                call_id,
                name: name.clone(),
                label,
                detail,
                action_index,
            });
            jobs.push((name, args));
        }
        let mut announced_rev = lock(handle).document.rev;
        let ctx = tool_ctx.clone();
        let tools_started = Instant::now();
        let (results, interrupted) = run_calls(
            jobs,
            run,
            |(name, _): &(String, Value)| tools::phase(name),
            move |(name, args): (String, Value)| {
                let ctx = ctx.clone();
                async move { tools::execute(&ctx, &name, args).await }
            },
            |i, result| {
                let p = &pending[i];
                log::debug!(
                    "round {round} tool {} finished {} ms after the round's tools started",
                    p.name,
                    tools_started.elapsed().as_millis()
                );
                let action = &mut actions.items[p.action_index];
                match result {
                    Ok(r) => {
                        action.status = ActionStatus::Done;
                        if r.cached {
                            action.detail = Some(match &p.detail {
                                Some(d) => format!("{d} · 이미 본 결과"),
                                None => "이미 본 결과".into(),
                            });
                        }
                    }
                    Err(_) if run.stop.load(Ordering::SeqCst) => mark_stopped(action, &p.label),
                    Err(e) => {
                        action.status = ActionStatus::Failed;
                        action.label = format!("{} · {}", p.label, error_label(&e.to_string()));
                    }
                }
                let rev = lock(handle).document.rev;
                if rev != announced_rev {
                    announced_rev = rev;
                    emit.send(AgentEventKind::DocumentChanged { rev });
                }
                actions.send(p.action_index);
                actions.save();
            },
        )
        .await;
        for (p, result) in pending.iter().zip(results) {
            let output = match result {
                Some(Ok(r)) => tool_output(&p.call_id, r, input.image_outputs_ok),
                Some(Err(e)) => tool_error(&p.call_id, &e.to_string()),
                None => {
                    if interrupted {
                        mark_stopped(&mut actions.items[p.action_index], &p.label);
                    } else {
                        actions.items[p.action_index].status = ActionStatus::Failed;
                    }
                    actions.send(p.action_index);
                    let why = if interrupted {
                        "사용자가 작업을 중단했어요"
                    } else {
                        "도구 실행이 중간에 끊겼어요"
                    };
                    tool_error(&p.call_id, why)
                }
            };
            input.items.push(output);
        }
        actions.save();
        interrupted
    }

    fn finish(
        &self,
        loop_result: anyhow::Result<()>,
        tool_ctx: &ToolCtx,
        facts: Option<&OriginalFacts>,
        input: Input,
        mut p: Progress<'_>,
    ) -> anyhow::Result<()> {
        let (handle, fonts, emit) = (&self.handle, &self.fonts, self.emit);
        let turn_state = &tool_ctx.state;
        if let Err(e) = &loop_result {
            log::error!("agent turn failed: {e:#}");
            for a in p.actions.items.iter_mut() {
                if matches!(a.status, ActionStatus::Running) {
                    a.status = ActionStatus::Failed;
                    a.label = format!("{} · 오류", a.label);
                    emit.send(AgentEventKind::Action { action: a.clone() });
                }
            }
            if !p.text.is_empty() {
                p.text.push_str("\n\n");
            }
            p.text.push_str(&format!(
                "오류로 작업을 멈췄어요: {}",
                error_label(&e.to_string())
            ));
        }
        p.billing.add(0.0);

        let report = if turn_state.edited() {
            Some(final_report(&lock(handle), fonts, facts, turn_state)?)
        } else {
            None
        };
        if let Some(r) = &report {
            emit.send(AgentEventKind::Report { report: r.clone() });
        }
        if p.stopped && p.text.is_empty() {
            p.text = "작업을 중단했어요.".into();
        }
        emit.send(AgentEventKind::Text { text: p.text.clone() });
        {
            let mut h = lock(handle);
            let chat_id = p.actions.chat_id;
            update_agent_chat(
                &h,
                |c| c.id == chat_id,
                |c| {
                    c.text = p.text.clone();
                    c.actions = p.actions.items;
                    c.report = report;
                    c.turn_krw = p.billing.total;
                    c.thinking_sec = p.thinking.as_secs_f64();
                },
            );
            input.save(&h);
            if turn_state.edited() {
                let thumb = img::thumbnail_png(&h, fonts).ok();
                let summary = p
                    .text
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("에이전트 작업")
                    .chars()
                    .take(40)
                    .collect::<String>();
                let _ = h.snapshot(Actor::Agent, &summary, Some(emit.turn.clone()), thumb.as_deref());
                if let Some(t) = &thumb {
                    let _ = h.write_thumbnail(t);
                }
            }
            let mut agent_state = load_agent_state(&h);
            agent_state["last_seq"] = json!(h.last_seq());
            agent_state["last_turn"] = json!(emit.turn);
            save_agent_state(&h, &agent_state);
        }
        if loop_result.is_ok() {
            emit.send(AgentEventKind::TurnEnd { stopped: p.stopped });
        }
        loop_result
    }
}

struct Progress<'a> {
    actions: ActionLog<'a>,
    billing: Billing<'a>,
    text: String,
    report_at: Option<Range<usize>>,
    stopped: bool,
    thinking: Duration,
}

impl Progress<'_> {
    fn replace_report(&mut self, before_round: usize) {
        let mut start = before_round;
        if let Some(old) = self.report_at.take() {
            self.text.replace_range(old.clone(), "");
            start -= old.len();
        }
        self.report_at = Some(start..self.text.len());
    }
}

struct ActionLog<'a> {
    items: Vec<ActionItem>,
    emit: &'a Emit,
    handle: &'a Arc<Mutex<ProjectHandle>>,
    chat_id: &'a str,
}

impl ActionLog<'_> {
    fn announce(&mut self, label: &str, detail: Option<String>, status: ActionStatus, round: usize) -> usize {
        self.items.push(ActionItem {
            id: uuid::Uuid::new_v4().simple().to_string(),
            label: label.to_string(),
            detail,
            status,
            round: u32::try_from(round).unwrap_or(u32::MAX),
        });
        let index = self.items.len() - 1;
        self.send(index);
        index
    }

    fn send(&self, index: usize) {
        self.emit.send(AgentEventKind::Action {
            action: self.items[index].clone(),
        });
    }

    fn save(&self) {
        let h = lock(self.handle);
        update_agent_chat(&h, |c| c.id == self.chat_id, |c| c.actions = self.items.clone());
    }
}

struct Billing<'a> {
    total: f64,
    images_billed: f64,
    turn_state: &'a TurnState,
    handle: &'a Arc<Mutex<ProjectHandle>>,
    emit: &'a Emit,
}

impl Billing<'_> {
    fn add(&mut self, text_krw: f64) {
        let images = *lock(&self.turn_state.cost_krw);
        let image_krw = images - self.images_billed;
        self.images_billed = images;
        if text_krw + image_krw <= 0.0 {
            return;
        }
        self.total += text_krw + image_krw;
        let project = {
            let mut h = lock(self.handle);
            if let Err(e) = h.add_usage(text_krw, image_krw) {
                log::warn!("project usage not saved: {e}");
            }
            h.project.clone()
        };
        self.emit.send(AgentEventKind::Usage {
            turn_krw: self.total,
            project_krw: project.usage_krw,
            text_krw: project.usage_text_krw,
            image_krw: project.usage_image_krw,
        });
    }
}

struct PendingCall {
    call_id: String,
    name: String,
    label: String,
    detail: Option<String>,
    action_index: usize,
}

/// Rebuilt only when the fit mode changes (the relayout skill joins once the user has answered), so the
/// cached prompt prefix stays the same for the rest of the turn.
pub(super) struct TurnPrompt {
    fit: FitMode,
    pub(super) text: String,
}

impl TurnPrompt {
    pub(super) fn build(h: &ProjectHandle, facts: Option<&OriginalFacts>, kind: TurnKind) -> Self {
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

fn final_report(
    h: &ProjectHandle,
    fonts: &Fonts,
    facts: Option<&OriginalFacts>,
    turn_state: &TurnState,
) -> anyhow::Result<Report> {
    match facts {
        Some(f) => {
            let compared = img::compare_full(h, fonts, f, COMPARE_MAX_PX)?;
            let dpi = hoist_core::check::dpi_summary(&h.document, h.project.target_dpi);
            let min_dpi = dpi.min_dpi.unwrap_or(h.project.target_dpi);
            let background = dpi
                .background_min_dpi
                .map(|b| format!(" · 배경 {}dpi", b.round()))
                .unwrap_or_default();
            let promoted = turn_state.promoted.load(Ordering::SeqCst);
            let summary = format!(
                "유사도 {:.0}% · 겹침 {:.0}% · 실물 {}dpi{background} {} · 방식 바꿈 {}",
                compared.result.similarity * 100.0,
                compared.result.overlap * 100.0,
                min_dpi.round(),
                if dpi.ok { "충족" } else { "부족" },
                promoted
            );
            Ok(Report {
                original_png: img::data_url(&compared.original_png),
                work_png: img::data_url(&compared.work_png),
                overlap: compared.result.overlap,
                similarity: compared.result.similarity,
                min_dpi,
                background_min_dpi: dpi.background_min_dpi,
                dpi_ok: Some(dpi.ok),
                promoted,
                summary,
            })
        }
        None => {
            let png = img::thumbnail_png(h, fonts)?;
            Ok(Report {
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
}

fn error_label(message: &str) -> String {
    message.chars().take(60).collect()
}

fn mark_stopped(action: &mut ActionItem, label: &str) {
    action.status = ActionStatus::Stopped;
    action.label = format!("{label} · 중단됨");
}

/// The user reads Korean only; a line the model slips into Japanese (kana) is dropped.
fn korean_only(text: &str) -> String {
    let kana = |c: char| ('\u{3040}'..='\u{30ff}').contains(&c);
    if !text.chars().any(kana) {
        return text.to_string();
    }
    text.lines()
        .filter(|l| !l.chars().any(kana))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn a_panicking_turn_ends_as_an_error() {
        let turn: anyhow::Result<()> = caught(async { panic!("boom") }).await;
        assert!(turn.is_err());
        assert_eq!(caught(async { Ok(7) }).await.ok(), Some(7));
    }

    #[test]
    fn kana_lines_are_dropped() {
        assert_eq!(korean_only("간격을 넓혔어요."), "간격을 넓혔어요.");
        assert_eq!(
            korean_only("行間が狭い部分を整えます。\n\n간격을 넓혔어요."),
            "간격을 넓혔어요."
        );
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
}
