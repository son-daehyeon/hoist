mod ask;
mod edit;
mod extract;
mod generate;
mod labels;
mod layout;
mod outpaint;
mod read;
mod schema;
mod text_box;
mod turn;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail};
use hoist_core::check::export_check;
use hoist_core::model::{Actor, Document, Layer, Op};
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;
use serde_json::{Value, json};

use ask::ask_user;
use edit::{add_layer, delete_layer, move_layer, outline_text, path_boolean, update_layer};
use extract::{crop_original, cut_out, trace_region};
use generate::{extend_background, generate_image};
pub use labels::{detail_for, label_for};
use layout::{arrange_row, fit_box};
use read::{
    analyze_original, compare_tool, document_summary, font_lineup, measure, view_original, view_work,
};
pub use schema::definitions;
pub use text_box::{fit_typed_text, ink_mm, union_mm};
pub use turn::{ReadMemo, TurnState};
use turn::{memo_key, set_fit_mode, set_plan};

use super::prompt::OriginalFacts;
use super::{AgentRun, Question};
use crate::openai::OpenAI;
use crate::state::lock;

#[derive(Clone)]
pub struct ToolCtx {
    pub handle: Arc<Mutex<ProjectHandle>>,
    pub fonts: Arc<Fonts>,
    pub openai: OpenAI,
    pub image_model: String,
    pub turn: String,
    pub facts: Option<OriginalFacts>,
    pub run: Arc<AgentRun>,
    pub state: Arc<TurnState>,
    pub on_question: Arc<dyn Fn(Question) + Send + Sync>,
    pub models_dir: PathBuf,
}

pub struct ToolResult {
    pub text: String,
    pub images: Vec<Vec<u8>>,
    pub cached: bool,
}

impl ToolResult {
    fn text(t: impl Into<String>) -> Self {
        ToolResult {
            cached: false,
            text: t.into(),
            images: vec![],
        }
    }
    fn json(v: Value) -> Self {
        ToolResult {
            cached: false,
            text: v.to_string(),
            images: vec![],
        }
    }
    fn with_image(v: Value, png: Vec<u8>) -> Self {
        ToolResult {
            cached: false,
            text: v.to_string(),
            images: vec![png],
        }
    }
}

fn with_fields(mut res: ToolResult, extra: Value) -> ToolResult {
    let mut v: Value = serde_json::from_str(&res.text).unwrap_or_else(|_| json!({}));
    if let (Some(m), Value::Object(extra)) = (v.as_object_mut(), extra) {
        m.extend(extra);
    }
    res.text = v.to_string();
    res
}

/// Run one after another, before the rest of the round, so their effects (fit mode, plan, memory, answers) apply to it.
const SEQUENTIAL_TOOLS: &[&str] = &[
    "set_fit_mode",
    "set_plan",
    "set_memory",
    "append_memory",
    "ask_user",
];

/// Run one after another, after the rest of the round and in this order, so they can name layers the round adds.
const LAST_TOOLS: &[&str] = &["arrange_row", "fit_box"];

/// 0: sequential before the round, 1: together, 2 and up: sequential after it, lowest first.
pub fn phase(name: &str) -> usize {
    if SEQUENTIAL_TOOLS.contains(&name) {
        0
    } else {
        LAST_TOOLS.iter().position(|t| *t == name).map_or(1, |i| i + 2)
    }
}

const MEMOIZED_TOOLS: &[&str] = &[
    "get_document",
    "view_original",
    "view_work",
    "font_lineup",
    "compare",
    "analyze_original",
    "measure",
    "crop_original",
    "cut_out",
];

const GATED_TOOLS: &[&str] = &[
    "add_layer",
    "update_layer",
    "delete_layer",
    "move_layer",
    "arrange_row",
    "fit_box",
    "path_boolean",
    "outline_text",
    "trace_region",
    "generate_image",
    "extend_background",
    "crop_original",
    "cut_out",
];

pub async fn execute(ctx: &ToolCtx, name: &str, args: Value) -> anyhow::Result<ToolResult> {
    check_gate(&ctx.state, name)?;
    match name {
        "generate_image" => generate_image(ctx, &args).await,
        "extend_background" => extend_background(ctx, &args).await,
        "ask_user" => ask_user(ctx, &args).await,
        _ => {
            let ctx = ctx.clone();
            let name = name.to_string();
            tokio::task::spawn_blocking(move || execute_sync(&ctx, &name, args)).await?
        }
    }
}

fn check_gate(state: &TurnState, name: &str) -> anyhow::Result<()> {
    if !GATED_TOOLS.contains(&name) {
        return Ok(());
    }
    if state.must_ask_first && !state.asked.load(Ordering::SeqCst) {
        bail!("The original changed: ask the user with ask_user how to proceed before any edit.");
    }
    if state.fit_pending.load(Ordering::SeqCst) {
        bail!(
            "Ask the fit mode first with ask_user ('배경만 늘려 채우기' / '재배치'); the answer is saved automatically, then edit."
        );
    }
    if state.plan_pending.load(Ordering::SeqCst) {
        bail!("Save the plan with set_plan (steps, image_quality) before editing.");
    }
    Ok(())
}

fn execute_sync(ctx: &ToolCtx, name: &str, args: Value) -> anyhow::Result<ToolResult> {
    if MEMOIZED_TOOLS.contains(&name) {
        let version = read_version(ctx, name, &args);
        let round = ctx.state.round.load(Ordering::SeqCst);
        return ctx.state.memo.run(
            memo_key(name, &args, version),
            round,
            |first, text| {
                if name == "get_document" {
                    format!("변경 없음 (rev {version}, 라운드 {first}와 동일)")
                } else {
                    format!(
                        "라운드 {first} 결과와 동일 (이미지는 다시 보내지 않음 — 다시 봐야 하면 영역이나 max_px를 바꿔 호출): {text}"
                    )
                }
            },
            || read_tool(ctx, name, &args),
        );
    }
    match name {
        "get_layer" => get_layer(ctx, &args),
        "add_layer" => add_layer(ctx, &args),
        "update_layer" => update_layer(ctx, &args),
        "path_boolean" => path_boolean(ctx, &args),
        "outline_text" => outline_text(ctx, &args),
        "delete_layer" => delete_layer(ctx, &args),
        "move_layer" => move_layer(ctx, &args),
        "arrange_row" => arrange_row(ctx, &args),
        "fit_box" => fit_box(ctx, &args),
        "trace_region" => trace_region(ctx, &args),
        "list_fonts" => list_fonts(ctx),
        "export_check" => export_check_tool(ctx, &args),
        "set_memory" => set_memory(ctx, &args),
        "append_memory" => append_memory(ctx, &args),
        "set_fit_mode" => set_fit_mode(ctx, &args),
        "set_plan" => set_plan(ctx, &args),
        _ => bail!("알 수 없는 도구 {name}"),
    }
}

fn read_version(ctx: &ToolCtx, name: &str, args: &Value) -> u64 {
    match name {
        "get_document" | "view_work" | "compare" => lock(&ctx.handle).document.rev,
        _ => version_of(ctx, args).map(u64::from).unwrap_or(0),
    }
}

fn read_tool(ctx: &ToolCtx, name: &str, args: &Value) -> anyhow::Result<ToolResult> {
    match name {
        "get_document" => Ok(ToolResult::json(document_summary(&lock(&ctx.handle).document))),
        "view_original" => view_original(ctx, args),
        "view_work" => view_work(ctx, args),
        "font_lineup" => font_lineup(ctx, args),
        "compare" => compare_tool(ctx, args),
        "analyze_original" => analyze_original(ctx, args),
        "measure" => measure(ctx, args),
        "crop_original" => crop_original(ctx, args),
        "cut_out" => cut_out(ctx, args),
        _ => bail!("알 수 없는 도구 {name}"),
    }
}

fn get_layer(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let id = arg_str(args, "id")?;
    let h = lock(&ctx.handle);
    let l = layer_by_id(&h.document, &id)?;
    Ok(ToolResult::json(serde_json::to_value(l)?))
}

fn list_fonts(ctx: &ToolCtx) -> anyhow::Result<ToolResult> {
    let fonts: Vec<Value> = ctx
        .fonts
        .catalog()
        .into_iter()
        .map(|f| {
            let mut o = json!({"family": f.family, "weights": f.weights});
            if let Some(n) = f.display_name {
                o["display_name"] = json!(n);
            }
            if let Some(c) = f.category {
                o["category"] = json!(c);
            }
            o
        })
        .collect();
    Ok(ToolResult::json(json!({"fonts": fonts})))
}

fn export_check_tool(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let h = lock(&ctx.handle);
    let dpi = args["dpi"].as_f64().unwrap_or(h.project.target_dpi);
    Ok(ToolResult::json(
        json!({"dpi": dpi, "warnings": export_check(&h.document, dpi)}),
    ))
}

fn set_memory(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let text = arg_str(args, "text")?;
    lock(&ctx.handle).set_memory(&text)?;
    Ok(ToolResult::text("메모리 저장"))
}

fn append_memory(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let line = arg_str(args, "line")?;
    let h = lock(&ctx.handle);
    let mut m = h.memory().unwrap_or_default();
    if !m.ends_with('\n') && !m.is_empty() {
        m.push('\n');
    }
    m.push_str(&format!("- {line}\n"));
    h.set_memory(&m)?;
    Ok(ToolResult::text("메모리에 추가"))
}

fn arg_str(args: &Value, key: &str) -> anyhow::Result<String> {
    args[key]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("{key}가 필요합니다"))
}

fn arg_rect_px(args: &Value, key: &str) -> anyhow::Result<[u32; 4]> {
    let a = args[key]
        .as_array()
        .ok_or_else(|| anyhow!("{key}는 [x,y,w,h]여야 합니다"))?;
    if a.len() != 4 {
        bail!("{key}는 [x,y,w,h]여야 합니다");
    }
    let mut r = [0u32; 4];
    for (i, v) in a.iter().enumerate() {
        r[i] = v
            .as_f64()
            .ok_or_else(|| anyhow!("{key}[{i}]가 숫자가 아닙니다"))?
            .max(0.0)
            .round() as u32;
    }
    if r[2] == 0 || r[3] == 0 {
        bail!("{key}의 w,h는 1 이상이어야 합니다");
    }
    Ok(r)
}

fn arg_rect_px_opt(args: &Value, key: &str) -> anyhow::Result<Option<[u32; 4]>> {
    if args[key].is_null() {
        return Ok(None);
    }
    Ok(Some(arg_rect_px(args, key)?))
}

fn arg_rect_mm(args: &Value, key: &str) -> Option<[f64; 4]> {
    let a = args[key].as_array()?;
    if a.len() != 4 {
        return None;
    }
    let mut r = [0f64; 4];
    for (i, v) in a.iter().enumerate() {
        r[i] = v.as_f64()?;
    }
    Some(r)
}

fn require_original(ctx: &ToolCtx) -> anyhow::Result<&OriginalFacts> {
    ctx.facts
        .as_ref()
        .ok_or_else(|| anyhow!("이 프로젝트에는 원본이 없습니다"))
}

fn version_of(ctx: &ToolCtx, args: &Value) -> anyhow::Result<u32> {
    match args["version"].as_u64() {
        Some(v) => Ok(v as u32),
        None => Ok(require_original(ctx)?.version),
    }
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn tidy_name(name: &str) -> String {
    name.replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn asset_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "asset".into() } else { s }
}

/// Tool tasks already running when the user stops would otherwise still edit the document behind a "failed" action.
fn ensure_running(ctx: &ToolCtx) -> anyhow::Result<()> {
    if ctx.run.stop.load(Ordering::SeqCst) {
        bail!("사용자가 작업을 중단해 적용하지 않았어요");
    }
    Ok(())
}

fn apply_as_agent(h: &mut ProjectHandle, turn: &str, ops: Vec<Op>) -> anyhow::Result<()> {
    h.apply_ops(ops, Actor::Agent, Some(turn.to_string()))?;
    Ok(())
}

fn layer_by_id<'a>(doc: &'a Document, id: &str) -> anyhow::Result<&'a Layer> {
    doc.layer(id).ok_or_else(|| anyhow!("레이어 {id} 없음"))
}

fn find_layer<'a>(doc: &'a Document, r: &str) -> anyhow::Result<&'a Layer> {
    doc.layer(r)
        .or_else(|| doc.layers.iter().find(|l| l.name == r))
        .or_else(|| doc.layers.iter().find(|l| l.name == tidy_name(r)))
        .ok_or_else(|| anyhow!("layer '{r}' not found: give a layer id or its exact name"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use hoist_core::model::*;
    use image::RgbaImage;

    use super::*;
    use crate::agent::prompt;

    pub(super) fn rect_layer(id: &str, x: f64) -> Layer {
        Layer {
            id: id.into(),
            name: id.into(),
            parent: None,
            kind: LayerKind::Shape(ShapeProps::Rect { rx_mm: 0.0 }),
            x_mm: x,
            y_mm: 50.0,
            w_mm: 100.0,
            h_mm: 50.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![Paint::Solid {
                color: "#FFFFFF".into(),
                opacity: 1.0,
            }],
            strokes: vec![],
            effects: vec![],
            strategy: Strategy::NativeVector,
            strategy_reason: String::new(),
            pinned: vec![],
            source: None,
        }
    }

    pub(super) fn test_ctx(models_dir: PathBuf) -> ToolCtx {
        let root = std::env::temp_dir().join(format!("hoist-cut-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root.clone())
            .create("cut", 600.0, 360.0)
            .expect("project");
        let original = root.join("disc.png");
        RgbaImage::from_fn(1000, 600, |x, y| {
            let (dx, dy) = ((f64::from(x) - 500.0) / 200.0, (f64::from(y) - 300.0) / 200.0);
            if dx * dx + dy * dy <= 1.0 {
                image::Rgba([230, 40, 40, 255])
            } else {
                image::Rgba([40, 160, 70, 255])
            }
        })
        .save(&original)
        .expect("original");
        h.add_original(&original).expect("add original");
        let facts = prompt::original_facts(&h.project);
        ToolCtx {
            handle: Arc::new(Mutex::new(h)),
            fonts: Arc::new(Fonts::new(None)),
            openai: OpenAI::new("test".into()).expect("client"),
            image_model: String::new(),
            turn: "t".into(),
            facts,
            run: Arc::new(AgentRun::new()),
            state: Arc::new(TurnState::default()),
            on_question: Arc::new(|_| {}),
            models_dir,
        }
    }

    #[test]
    fn edits_wait_for_the_fit_mode_answer() {
        let state = TurnState {
            fit_pending: AtomicBool::new(true),
            ..Default::default()
        };
        let err = check_gate(&state, "add_layer")
            .expect_err("edit before the answer")
            .to_string();
        assert!(err.contains("ask_user") && err.contains("fit mode"), "{err}");
        assert!(check_gate(&state, "trace_region").is_err());
        assert!(check_gate(&state, "view_original").is_ok());
        assert!(check_gate(&state, "ask_user").is_ok());
        assert!(check_gate(&state, "set_fit_mode").is_ok());
        state.asked.store(true, Ordering::SeqCst);
        assert!(
            check_gate(&state, "add_layer").is_err(),
            "asking alone is not enough"
        );
        state.fit_pending.store(false, Ordering::SeqCst);
        assert!(check_gate(&state, "add_layer").is_ok());

        let mut project = Project {
            id: "p".into(),
            name: "p".into(),
            created_at: String::new(),
            updated_at: String::new(),
            width_mm: 6000.0,
            height_mm: 900.0,
            target_dpi: 100.0,
            originals: vec![],
            fit_mode: FitMode::Ask,
            usage_krw: 0.0,
            usage_text_krw: 0.0,
            usage_image_krw: 0.0,
        };
        let original = |w: u32, h: u32| OriginalFacts {
            version: 1,
            width_px: w,
            height_px: h,
            mm_per_px: 1.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
        };
        assert!(prompt::needs_fit_answer(&project, Some(&original(1774, 470))));
        assert!(!prompt::needs_fit_answer(&project, Some(&original(2000, 300))));
        assert!(!prompt::needs_fit_answer(&project, None));
        project.fit_mode = FitMode::Relayout;
        assert!(!prompt::needs_fit_answer(&project, Some(&original(1774, 470))));
    }

    #[test]
    fn every_defined_tool_is_wired_to_an_executor() {
        let ctx = test_ctx(std::env::temp_dir().join("hoist-no-models"));
        let async_tools = ["generate_image", "extend_background", "ask_user"];
        for def in definitions() {
            let name = def["name"].as_str().expect("tool name").to_string();
            if async_tools.contains(&name.as_str()) {
                continue;
            }
            if let Err(e) = execute_sync(&ctx, &name, json!({})) {
                assert!(!e.to_string().contains("알 수 없는 도구"), "{name}: {e}");
            }
        }
    }
}
