use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

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
pub(super) struct Emit {
    pub(super) channel: Channel<AgentEvent>,
    pub(super) project_id: String,
    pub(super) turn: String,
}

impl Emit {
    pub(super) fn send(&self, event: AgentEventKind) {
        let _ = self.channel.send(AgentEvent {
            project_id: self.project_id.clone(),
            turn: self.turn.clone(),
            event,
        });
    }
}
