use hoist_core::store::ProjectHandle;
use serde::{Deserialize, Serialize};

use super::events::{ActionItem, Question, Report, SendContext};

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

pub(super) fn open_turn(
    h: &ProjectHandle,
    agent_item_id: &str,
    text: String,
    context: SendContext,
    edit_summary: Vec<String>,
) -> anyhow::Result<()> {
    let user = ChatItem::User(UserChat {
        id: uuid::Uuid::new_v4().simple().to_string(),
        ts: now(),
        text,
        context,
        edit_summary,
    });
    let agent = ChatItem::Agent(Box::new(AgentChat {
        id: agent_item_id.to_string(),
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
    h.append_chat(&serde_json::to_value(&user)?)?;
    h.append_chat(&serde_json::to_value(&agent)?)?;
    Ok(())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::events::AUTO_LABEL;
    use super::*;

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
