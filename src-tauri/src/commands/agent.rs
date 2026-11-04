use std::sync::atomic::Ordering;

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::agent::{self, AgentEvent, SendContext, img, prompt::TurnKind};
use crate::error::{Error, Result};
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn agent_send(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    text: String,
    context: SendContext,
    on_event: Channel<AgentEvent>,
) -> Result<String> {
    if state.settings_snapshot().openai_key().is_none() {
        return Err(Error::NoApiKey);
    }
    let h = state.handle(&id)?;
    let kind = {
        let g = img::lock(&h);
        if agent::is_first_turn(&g) {
            if g.project.originals.is_empty() {
                TurnKind::Scratch
            } else {
                TurnKind::Reconstruct
            }
        } else {
            TurnKind::Normal
        }
    };
    if let Some(run) = state.agents.lock()?.get(&id)
        && run.running.load(Ordering::SeqCst)
    {
        return Err(Error::Invalid("도우미가 아직 작업 중이에요".into()));
    }
    agent::spawn_turn(app, id.clone(), text, context, kind, on_event);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub fn agent_stop(state: State<'_, AppState>, id: String) -> Result<()> {
    if let Some(run) = state.agents.lock()?.get(&id) {
        run.request_stop();
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn agent_answer(
    state: State<'_, AppState>,
    id: String,
    question_id: String,
    answer: String,
) -> Result<()> {
    let delivered = state
        .agents
        .lock()?
        .get(&id)
        .map(|r| r.answer(&question_id, &answer))
        .unwrap_or(false);
    if !delivered {
        return Err(Error::Invalid("답할 질문이 없어요".into()));
    }
    if let Ok(h) = state.handle(&id) {
        let g = img::lock(&h);
        agent::update_agent_chat(
            &g,
            |c| c.question.as_ref().is_some_and(|q| q.id == question_id),
            |c| c.answer = Some(answer),
        );
    }
    Ok(())
}
