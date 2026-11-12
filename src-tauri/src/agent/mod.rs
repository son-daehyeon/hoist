mod calls;
mod chat;
mod classify;
mod context;
mod edit_summary;
mod events;
mod guards;
mod img;
mod layer_args;
pub mod prompt;
mod relayout_guard;
pub mod tools;
mod turn;
mod user_message;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

pub use chat::{ChatItem, chat_items, update_agent_chat};
pub use classify::TurnClass;
pub use edit_summary::describe_entry;
pub use events::{AUTO_ANSWER, AUTO_LABEL, AgentEvent, Question, SendContext};
pub use turn::{spawn_turn, turn_kind};

use crate::state::lock;

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

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.stop_notify.notify_waiters();
    }

    pub fn answer(&self, question_id: &str, answer: &str) -> bool {
        lock(&self.pending)
            .remove(question_id)
            .is_some_and(|tx| tx.send(answer.to_string()).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_reaches_only_its_waiting_question() {
        let run = AgentRun::new();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        lock(&run.pending).insert("q9".into(), tx);
        assert!(!run.answer("q8", "x"), "unknown question");
        assert!(run.answer("q9", "y"));
        assert_eq!(rx.try_recv().ok().as_deref(), Some("y"));
    }
}
