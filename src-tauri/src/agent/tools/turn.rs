use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::{anyhow, bail};
use hoist_core::model::{FitMode, Layer, LayerKind};
use hoist_core::ops::merge_patch;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ToolCtx, ToolResult};
use crate::agent::context::{load_agent_state, save_agent_state};
use crate::agent::prompt;
use crate::state::lock;

#[derive(Default)]
pub struct TurnState {
    pub asked: AtomicBool,
    pub must_ask_first: bool,
    pub fit_pending: AtomicBool,
    pub edits: AtomicUsize,
    pub promoted: AtomicUsize,
    pub cost_krw: Mutex<f64>,
    pub round: AtomicUsize,
    pub memo: ReadMemo,
    pub plan: Mutex<Option<Plan>>,
    pub plan_pending: AtomicBool,
    pub wording_edits: AtomicUsize,
}

impl TurnState {
    /// A comparison with the original would only measure the wording the user asked to change.
    pub fn wording_only(&self) -> bool {
        let edits = self.edits.load(Ordering::SeqCst);
        edits > 0 && self.wording_edits.load(Ordering::SeqCst) == edits
    }

    pub fn edited(&self) -> bool {
        self.edits.load(Ordering::SeqCst) > 0
    }
}

/// Judged against the layer: the strict schema makes the model resend the whole `kind`.
pub(super) fn wording_only_patch(layer: &Layer, patch: &Value) -> bool {
    let Ok(mut merged) = serde_json::to_value(layer) else {
        return false;
    };
    merge_patch(&mut merged, patch);
    let Ok(mut after) = serde_json::from_value::<Layer>(merged) else {
        return false;
    };
    let (LayerKind::Text(old), LayerKind::Text(new)) = (&layer.kind, &mut after.kind) else {
        return false;
    };
    if old.text == new.text {
        return false;
    }
    new.text.clone_from(&old.text);
    after == *layer
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Plan {
    pub steps: Vec<String>,
    pub image_quality: PlanQuality,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PlanQuality {
    pub background: Quality,
    pub figure: Quality,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    fn as_str(self) -> &'static str {
        match self {
            Quality::Low => "low",
            Quality::Medium => "medium",
            Quality::High => "high",
        }
    }
}

#[derive(Default)]
pub struct ReadMemo(Mutex<HashMap<String, MemoEntry>>);

struct MemoEntry {
    round: usize,
    text: String,
    had_images: bool,
}

impl ReadMemo {
    pub fn run(
        &self,
        key: String,
        round: usize,
        on_hit: impl FnOnce(usize, &str) -> String,
        exec: impl FnOnce() -> anyhow::Result<ToolResult>,
    ) -> anyhow::Result<ToolResult> {
        let seen = lock(&self.0).get(&key).map(|e| (e.round, e.text.clone()));
        if let Some((first_round, text)) = seen {
            return Ok(ToolResult {
                text: on_hit(first_round, &text),
                images: vec![],
                cached: true,
            });
        }
        let result = exec()?;
        lock(&self.0).insert(
            key,
            MemoEntry {
                round,
                text: result.text.clone(),
                had_images: !result.images.is_empty(),
            },
        );
        Ok(result)
    }

    pub fn forget_images(&self) {
        lock(&self.0).retain(|_, e| !e.had_images);
    }
}

pub(super) fn memo_key(name: &str, args: &Value, version: u64) -> String {
    let mut args = args.clone();
    if let Some(fields) = args.as_object_mut() {
        fields.retain(|_, v| !v.is_null());
    }
    format!("{name}|{version}|{args}")
}

pub(super) fn set_fit_mode(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let mode: FitMode = serde_json::from_value(args["mode"].clone())
        .map_err(|_| anyhow!("mode는 extend_background 또는 relayout이어야 합니다"))?;
    lock(&ctx.handle).set_fit_mode(mode)?;
    ctx.state.fit_pending.store(false, Ordering::SeqCst);
    Ok(ToolResult::text(prompt::fit_mode_instruction(mode)))
}

pub(super) fn set_plan(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let plan: Plan = serde_json::from_value(args.clone()).map_err(|e| anyhow!("set_plan 인자 오류: {e}"))?;
    if plan.steps.is_empty() {
        bail!("steps에 할 일을 하나 이상 넣으세요");
    }
    {
        let h = lock(&ctx.handle);
        let mut st = load_agent_state(&h);
        st["plan"] = json!({"turn": ctx.turn, "plan": plan});
        save_agent_state(&h, &st);
    }
    let text = format!(
        "계획을 저장했습니다({}단계). 다음 라운드부터 실행 모델로 진행합니다. 그림 기본 품질 {}, 배경 기본 품질 {}.",
        plan.steps.len(),
        plan.image_quality.figure.as_str(),
        plan.image_quality.background.as_str()
    );
    *lock(&ctx.state.plan) = Some(plan);
    ctx.state.plan_pending.store(false, Ordering::SeqCst);
    Ok(ToolResult::text(text))
}

pub(super) fn plan_quality(ctx: &ToolCtx, pick: fn(&PlanQuality) -> Quality) -> Option<String> {
    lock(&ctx.state.plan)
        .as_ref()
        .map(|p| pick(&p.image_quality).as_str().to_string())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::agent::layer_args;
    use crate::agent::tools::tests::test_ctx;
    use crate::agent::tools::{check_gate, execute_sync};

    fn shown(text: &str) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            text: text.into(),
            images: vec![vec![1, 2, 3]],
            cached: false,
        })
    }

    #[test]
    fn read_memo_skips_the_same_call() {
        let memo = ReadMemo::default();
        let runs = Cell::new(0);
        let exec = || {
            runs.set(runs.get() + 1);
            shown("원본 v1")
        };
        let hit = |first: usize, text: &str| format!("라운드 {first}: {text}");
        let key = memo_key(
            "view_original",
            &json!({"rect_px": [0, 0, 10, 10], "max_px": null}),
            1,
        );
        let first = memo.run(key, 3, hit, exec).expect("first call");
        assert!(!first.cached);
        assert_eq!(first.images.len(), 1);

        let same = memo_key("view_original", &json!({"rect_px": [0, 0, 10, 10]}), 1);
        let again = memo.run(same, 5, hit, exec).expect("repeat call");
        assert_eq!(runs.get(), 1);
        assert!(again.cached);
        assert!(again.images.is_empty());
        assert_eq!(again.text, "라운드 3: 원본 v1");

        let other_version = memo_key("view_original", &json!({"rect_px": [0, 0, 10, 10]}), 2);
        memo.run(other_version, 6, hit, exec).expect("new version");
        assert_eq!(runs.get(), 2);

        memo.forget_images();
        let after_forget = memo_key("view_original", &json!({"rect_px": [0, 0, 10, 10]}), 1);
        assert!(!memo.run(after_forget, 7, hit, exec).expect("after forget").cached);
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn only_a_wording_patch_counts_as_wording() {
        let raw = json!({"name": "제목", "kind": {"type": "text", "text": "주민역량강화", "font_family": "Pretendard", "font_weight": 700, "font_size_mm": 100, "line_height": 1.2, "align": "left"}, "x_mm": 100, "y_mm": 50, "w_mm": 900, "h_mm": 300});
        let layer = layer_args::parse_layer(&raw, (6000.0, 900.0)).expect("text layer");
        let mut resent = serde_json::to_value(&layer.kind).expect("kind");
        resent["text"] = json!("주민사기강화");
        let wording =
            layer_args::patch_from_args(&json!({"name": null, "kind": resent, "x_mm": null, "fills": null}));
        assert!(wording_only_patch(&layer, &wording), "the whole kind resent");
        assert!(wording_only_patch(
            &layer,
            &json!({"kind": {"type": "text", "text": "주민사기강화"}})
        ));
        assert!(!wording_only_patch(
            &layer,
            &json!({"kind": {"type": "text", "text": "a", "font_size_mm": 120}})
        ));
        assert!(!wording_only_patch(
            &layer,
            &json!({"kind": {"type": "text", "text": "a"}, "x_mm": 10})
        ));
        assert!(!wording_only_patch(&layer, &json!({"kind": {"type": "text"}})));
        assert!(!wording_only_patch(&layer, &json!({"x_mm": 10})));
        let state = TurnState {
            edits: AtomicUsize::new(2),
            wording_edits: AtomicUsize::new(2),
            ..Default::default()
        };
        assert!(state.wording_only());
        state.edits.fetch_add(1, Ordering::SeqCst);
        assert!(!state.wording_only());
        state.edits.store(0, Ordering::SeqCst);
        state.wording_edits.store(0, Ordering::SeqCst);
        assert!(!state.wording_only(), "no edits is not a wording-only turn");
    }

    #[test]
    fn set_plan_unlocks_edits_and_sets_image_quality() {
        let ctx = test_ctx(std::env::temp_dir().join("hoist-no-models"));
        ctx.state.plan_pending.store(true, Ordering::SeqCst);
        let err = check_gate(&ctx.state, "add_layer")
            .expect_err("no plan yet")
            .to_string();
        assert!(err.contains("set_plan"), "{err}");
        assert!(check_gate(&ctx.state, "view_original").is_ok());
        assert!(check_gate(&ctx.state, "set_plan").is_ok());
        let quality = json!({"background": "medium", "figure": "high"});
        assert!(
            execute_sync(
                &ctx,
                "set_plan",
                json!({"steps": ["배경"], "image_quality": {"background": "max", "figure": "high"}})
            )
            .is_err(),
            "unknown quality"
        );
        execute_sync(
            &ctx,
            "set_plan",
            json!({"steps": ["배경", "글자"], "effort": "medium", "image_quality": quality, "notes": null}),
        )
        .expect("plan");
        assert!(check_gate(&ctx.state, "add_layer").is_ok());
        assert_eq!(plan_quality(&ctx, |q| q.figure).as_deref(), Some("high"));
        assert_eq!(plan_quality(&ctx, |q| q.background).as_deref(), Some("medium"));
        let saved = load_agent_state(&lock(&ctx.handle));
        assert_eq!(saved["plan"]["plan"]["image_quality"]["figure"], "high");
        assert_eq!(saved["plan"]["turn"], "t");
    }
}
