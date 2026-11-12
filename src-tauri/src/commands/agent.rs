use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use hoist_core::model::Project;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::agent::prompt::TurnKind;
use crate::agent::{self, AgentEvent, AgentRun, SendContext};
use crate::error::{Error, Result};
use crate::state::{AppState, lock};

fn require_api_key(state: &AppState) -> Result<()> {
    if state.settings_snapshot().openai_key().is_none() {
        return Err(Error::NoApiKey);
    }
    Ok(())
}

fn refuse_running(agents: &HashMap<String, Arc<AgentRun>>, id: &str) -> Result<()> {
    if agents.get(id).is_some_and(|run| run.is_running()) {
        return Err(Error::Invalid("에이전트가 아직 작업 중이에요".into()));
    }
    Ok(())
}

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
    require_api_key(&state)?;
    let h = state.handle(&id)?;
    let kind = {
        let g = lock(&h);
        agent::turn_kind(
            g.document.layers.len(),
            !g.project.originals.is_empty(),
            agent::TurnClass::from_keywords(&text),
        )
    };
    refuse_running(&*state.agents.lock()?, &id)?;
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
        let g = lock(&h);
        agent::update_agent_chat(
            &g,
            |c| c.has_question(&question_id),
            |c| c.record_answer(&question_id, &answer),
        );
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn replace_original(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    path: String,
    on_event: Channel<AgentEvent>,
) -> Result<Project> {
    require_api_key(&state)?;
    refuse_running(&*state.agents.lock()?, &id)?;
    let h = state.handle(&id)?;
    let project = {
        let mut g = lock(&h);
        g.add_original(Path::new(&path))?;
        g.project.clone()
    };
    agent::spawn_turn(
        app,
        id,
        "원본을 새 이미지로 바꿨어요.".into(),
        SendContext::default(),
        TurnKind::ReplaceOriginal,
        on_event,
    );
    Ok(project)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    #[test]
    fn a_running_turn_refuses_a_second_start() {
        let mut agents = HashMap::new();
        assert!(refuse_running(&agents, "p").is_ok());
        let run = Arc::new(AgentRun::new());
        agents.insert("p".to_string(), run.clone());
        assert!(refuse_running(&agents, "p").is_ok());
        run.running.store(true, Ordering::SeqCst);
        assert!(matches!(refuse_running(&agents, "p"), Err(Error::Invalid(_))));
        assert!(refuse_running(&agents, "q").is_ok());
    }
}
