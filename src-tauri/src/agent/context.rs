use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use hoist_core::store::ProjectHandle;
use serde_json::{Value, json};

use super::prompt::OriginalFacts;
use super::{img, tools};
use crate::state::lock;

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

pub(super) fn load_agent_state(h: &ProjectHandle) -> Value {
    std::fs::read(agent_state_path(h))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(json!({}))
}

pub(super) fn save_agent_state(h: &ProjectHandle, v: &Value) {
    let _ = std::fs::write(agent_state_path(h), serde_json::to_vec(v).unwrap_or_default());
}

pub(super) const DEMOTE_AT_INPUT_TOKENS: u64 = 300_000;
pub(super) const KEEP_IMAGES: usize = 4;
pub(super) const KEEP_IMAGES_ON_OVERFLOW: usize = 2;
const SIMPLE_TURN_CHAT_MESSAGES: usize = 6;

pub(super) struct Input {
    pub(super) items: Vec<Value>,
    pub(super) tools: Vec<Value>,
    pub(super) strict_tools: bool,
    pub(super) image_outputs_ok: bool,
    pub(super) overflow_stage: u8,
    pub(super) seen: HashMap<String, usize>,
    pub(super) last_input_tokens: u64,
    pub(super) kept: Vec<Value>,
    pub(super) sent: usize,
}

impl Input {
    pub(super) fn load(h: &ProjectHandle, simple: bool) -> Input {
        // Pictures from earlier turns are stale (this turn sends the current ones); their text stays.
        let mut items = forget_images(load_items(h));
        // A simple edit sends only the last few chat messages; the saved history stays whole.
        let mut kept = Vec::new();
        if simple {
            let chat: Vec<Value> = items
                .iter()
                .filter(|it| it["type"] == "message")
                .cloned()
                .collect();
            kept = std::mem::replace(
                &mut items,
                chat[chat.len().saturating_sub(SIMPLE_TURN_CHAT_MESSAGES)..].to_vec(),
            );
        }
        Input {
            sent: items.len(),
            items,
            kept,
            tools: tools::definitions(),
            strict_tools: true,
            image_outputs_ok: true,
            overflow_stage: 0,
            seen: HashMap::new(),
            last_input_tokens: 0,
        }
    }

    pub(super) fn demote(
        &mut self,
        keep: usize,
        handle: &Arc<Mutex<ProjectHandle>>,
        facts: Option<&OriginalFacts>,
        memo: &tools::ReadMemo,
    ) -> usize {
        let h = lock(handle);
        let demoted = demote_images(&mut self.items, keep, |call| {
            omitted_image(call, &h, facts, &self.seen)
        });
        if demoted > 0 {
            memo.forget_images();
        }
        demoted
    }

    pub(super) fn compacted(&mut self, items: Vec<Value>) {
        if !self.kept.is_empty() {
            let from = self.sent.min(self.items.len());
            self.kept.extend(self.items.drain(from..));
        }
        self.sent = items.len();
        self.items = items;
    }

    pub(super) fn save(self, h: &ProjectHandle) {
        if self.kept.is_empty() {
            save_items(h, &self.items);
        } else {
            let mut whole = self.kept;
            whole.extend_from_slice(&self.items[self.sent.min(self.items.len())..]);
            save_items(h, &whole);
        }
    }
}

fn omitted_image(
    call: Option<(&str, &Value, &str)>,
    h: &ProjectHandle,
    facts: Option<&OriginalFacts>,
    seen: &HashMap<String, usize>,
) -> String {
    let Some((name, args, call_id)) = call else {
        return "(image omitted: an image attached to the message, already seen — call view_original or view_work if you need it again)"
            .into();
    };
    let label = tools::label_for(name, args);
    let what = match tools::detail_for(name, args, &h.document, facts) {
        Some(d) => format!("{label} {d}"),
        None => label,
    };
    let when = seen
        .get(call_id)
        .map(|r| format!("seen in round {r}"))
        .unwrap_or_else(|| "seen in an earlier turn".into());
    format!(
        "(image omitted: {what}, {when} — call the same tool again with another region or zoom if you need it)"
    )
}

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

pub(super) fn input_image(png: &[u8]) -> Value {
    json!({"type": "input_image", "image_url": img::data_url(png), "detail": "high"})
}

pub(super) fn user_text(text: &str) -> Value {
    json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]})
}

pub(super) fn tool_error(call_id: &str, message: &str) -> Value {
    json!({"type": "function_call_output", "call_id": call_id, "output": json!({"error": message}).to_string()})
}

pub(super) fn tool_output(call_id: &str, r: tools::ToolResult, images_ok: bool) -> Value {
    if r.images.is_empty() || !images_ok {
        return json!({"type": "function_call_output", "call_id": call_id, "output": r.text});
    }
    let mut parts = vec![json!({"type": "input_text", "text": r.text})];
    for png in r.images {
        parts.push(input_image(&png));
    }
    json!({"type": "function_call_output", "call_id": call_id, "output": parts})
}

pub(super) fn forget_images(items: Vec<Value>) -> Vec<Value> {
    strip_image_outputs(items)
        .into_iter()
        .map(|mut it| {
            if let Some(parts) = it["content"].as_array()
                && parts.iter().any(|p| p["type"] == "input_image")
            {
                let mut kept: Vec<Value> = parts
                    .iter()
                    .filter(|p| p["type"] != "input_image")
                    .cloned()
                    .collect();
                if kept.is_empty() {
                    kept.push(json!({"type": "input_text", "text": "(earlier picture omitted)"}));
                }
                it["content"] = json!(kept);
            }
            it
        })
        .collect()
}

pub(super) fn strip_image_outputs(items: Vec<Value>) -> Vec<Value> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earlier_pictures_are_forgotten() {
        let items = vec![
            json!({"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "고쳐 주세요"},
                {"type": "input_image", "image_url": "data:image/png;base64,AAAA"}
            ]}),
            json!({"type": "function_call_output", "call_id": "c", "output": [
                {"type": "input_text", "text": "결과"},
                {"type": "input_image", "image_url": "data:image/png;base64,BBBB"}
            ]}),
        ];
        let kept = serde_json::to_string(&forget_images(items)).expect("json");
        assert!(!kept.contains("base64"), "{kept}");
        assert!(kept.contains("고쳐 주세요") && kept.contains("결과"), "{kept}");
    }

    #[test]
    fn a_compacted_simple_turn_still_saves_the_whole_history() {
        let root = std::env::temp_dir().join(format!("hoist-items-{}", uuid::Uuid::new_v4().simple()));
        let h = hoist_core::store::ProjectStore::new(root)
            .create("items", 600.0, 300.0)
            .expect("project");
        let call = |id: &str| json!({"type": "function_call", "call_id": id, "name": "get_document", "arguments": "{}"});
        let mut history: Vec<Value> = (0..8).map(|i| user_text(&format!("말 {i}"))).collect();
        history.insert(3, call("old"));
        history.insert(4, tool_error("old", "실패"));
        save_items(&h, &history);

        let mut input = Input::load(&h, true);
        assert_eq!(input.items.len(), SIMPLE_TURN_CHAT_MESSAGES);
        let before = [user_text("고쳐 주세요"), call("c1"), tool_error("c1", "실패")];
        input.items.extend(before.iter().cloned());
        input.compacted(vec![json!({"type": "compaction"})]);
        let after = [call("c2"), tool_error("c2", "실패")];
        input.items.extend(after.iter().cloned());
        input.save(&h);

        let saved = load_items(&h);
        let whole: Vec<Value> = history.into_iter().chain(before).chain(after).collect();
        assert_eq!(saved, whole);
        for output in saved.iter().filter(|it| it["type"] == "function_call_output") {
            assert!(
                saved
                    .iter()
                    .any(|it| it["type"] == "function_call" && it["call_id"] == output["call_id"]),
                "{output}"
            );
        }
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
}
