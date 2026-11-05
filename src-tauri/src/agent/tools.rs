use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail};
use hoist_core::boolean::{self, BooleanOp};
use hoist_core::check::export_check;
use hoist_core::model::*;
use hoist_core::render::layer_bounds_mm;
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;
use hoist_core::{analyze, image as cimg, matting, trace};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::prompt::{self, OriginalFacts};
use super::{AUTO_ANSWER, AUTO_LABEL, AgentRun, Question};
use super::{img, layer_args};
use crate::openai::{ImageRequest, OpenAI, image_cost_krw};

pub struct TurnState {
    pub asked: AtomicBool,
    pub must_ask_first: bool,
    pub fit_pending: AtomicBool,
    pub edits: AtomicUsize,
    pub promoted: AtomicUsize,
    pub cost_krw: Mutex<f64>,
    pub memory_dirty: AtomicBool,
    pub round: AtomicUsize,
    pub memo: ReadMemo,
    pub plan: Mutex<Option<Plan>>,
    pub plan_pending: AtomicBool,
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

/// Run one after another, before the rest of the round, so their effects (fit mode, plan, memory, answers) apply to it.
pub const SEQUENTIAL_TOOLS: &[&str] = &[
    "set_fit_mode",
    "set_plan",
    "set_memory",
    "append_memory",
    "ask_user",
];

/// Run one after another, after the rest of the round and in this order, so they can name layers the round adds.
pub const LAST_TOOLS: &[&str] = &["arrange_row", "fit_box"];

/// 0: sequential before the round, 1: together, 2 and up: sequential after it, lowest first.
pub fn phase(name: &str) -> usize {
    if SEQUENTIAL_TOOLS.contains(&name) {
        0
    } else {
        LAST_TOOLS.iter().position(|t| *t == name).map_or(1, |i| i + 2)
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
        let seen = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .map(|e| (e.round, e.text.clone()));
        if let Some((first_round, text)) = seen {
            return Ok(ToolResult {
                text: on_hit(first_round, &text),
                images: vec![],
                cached: true,
            });
        }
        let result = exec()?;
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(
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
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, e| !e.had_images);
    }
}

pub fn memo_key(name: &str, args: &Value, version: u64) -> String {
    let mut args = args.clone();
    if let Some(fields) = args.as_object_mut() {
        fields.retain(|_, v| !v.is_null());
    }
    format!("{name}|{version}|{args}")
}

const READ_TOOLS: &[&str] = &[
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
}

const EDIT_TOOLS: &[&str] = &[
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

pub fn definitions() -> Vec<Value> {
    let f = |name: &str, desc: &str, props: Value, required: &[&str]| json!({"type":"function","name":name,"description":desc,"parameters":{"type":"object","properties":props,"required":required,"additionalProperties":false}});
    let rect_px = json!({"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4,"description":"[x, y, w, h] in original pixels"});
    let rect_mm = json!({"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"[x, y, w, h] in mm"});
    vec![
        f(
            "get_document",
            "Returns the current document: artboard and a summary of every layer.",
            json!({}),
            &[],
        ),
        f(
            "get_layer",
            "Returns the full JSON of one layer.",
            json!({"id":{"type":"string"}}),
            &["id"],
        ),
        f(
            "view_original",
            "Shows the original image, or a zoomed part of it when rect_px is given.",
            json!({"version":{"type":["integer","null"]},"rect_px":{"anyOf":[rect_px.clone(),{"type":"null"}]},"max_px":{"type":["integer","null"],"description":"longest edge in px (default 1536)"}}),
            &[],
        ),
        f(
            "view_work",
            "Renders the current document. rect_mm limits the region, layer_ids limits the layers.",
            json!({"rect_mm":{"anyOf":[rect_mm.clone(),{"type":"null"}]},"layer_ids":{"type":["array","null"],"items":{"type":"string"}},"max_px":{"type":["integer","null"]}}),
            &[],
        ),
        f(
            "compare",
            "Shows the original and the work side by side and returns scores (similarity 0–1, overlap, edge_similarity) plus worst_regions (fractional coordinates 0–1).",
            json!({"rect_mm":{"anyOf":[rect_mm.clone(),{"type":"null"}]}}),
            &[],
        ),
        f(
            "analyze_original",
            "Returns the original's palette and candidate regions (text / shape / photo / background, bbox_px).",
            json!({"version":{"type":["integer","null"]}}),
            &[],
        ),
        f(
            "measure",
            "Measures a region of the original: dominant color, palette, and the bbox of one color.",
            json!({"rect_px":rect_px.clone(),"version":{"type":["integer","null"]},"color":{"type":["string","null"],"description":"#RRGGBB whose bbox to measure"},"tolerance":{"type":["integer","null"]}}),
            &["rect_px"],
        ),
        layer_args::add_layer_tool(true),
        layer_args::update_layer_tool(true),
        f(
            "delete_layer",
            "Deletes a layer (and its children if it is a group).",
            json!({"id":{"type":"string"}}),
            &["id"],
        ),
        f(
            "move_layer",
            "Changes a layer's parent or z-order. parent takes a group id or name (created when missing); index 0 is the bottom.",
            json!({"id":{"type":"string"},"parent":{"type":["string","null"]},"index":{"type":"integer"}}),
            &["id", "index"],
        ),
        json!({"type":"function","name":"arrange_row","strict":true,
            "description":"Lays items out left to right on one line using their real ink boxes (glyph outlines for text, the box for shapes and pictures), so gaps are exact instead of guessed. Each item is one layer or several layers that move together (e.g. the badge, its label and the body text of one info column). Only x changes; y, sizes and fonts stay. justify \"center\" centers the row in [x0_mm, x1_mm] with gap_mm between items; \"start\" and \"end\" push it to that side with gap_mm; \"space_between\" spreads the items across the whole range with equal gaps (gap_mm ignored). Returns each item's ink box, the content width, the gap used, the row extent and the x of every gap center (for dividers). When the content is wider than the range it reports overflow: reduce the font size or re-break lines and call again. Layers are given by id or exact name, so send it in the same round as the add_layer calls that create them; it runs after them.",
            "parameters":{"type":"object","additionalProperties":false,"required":["items","x0_mm","x1_mm","justify","gap_mm"],"properties":{
                "items":{"type":"array","items":{"type":"array","items":{"type":"string"}},"description":"left to right; each item lists the ids or names of the layers that move together"},
                "x0_mm":{"type":"number"},
                "x1_mm":{"type":"number"},
                "justify":{"type":"string","enum":["center","start","end","space_between"]},
                "gap_mm":{"type":["number","null"],"description":"gap between neighboring items; null only with space_between"}}}}),
        json!({"type":"function","name":"fit_box","strict":true,
            "description":"Moves and resizes a shape layer so it hugs the ink boxes of the given layers plus padding — for pills, bands, badges and panels that must be exactly as large as their content. axis \"x\" changes only x and width, \"y\" only y and height, \"both\" all four. Returns the new box. Layers are given by id or exact name; when sent in the same round it runs after add_layer and arrange_row.",
            "parameters":{"type":"object","additionalProperties":false,"required":["id","around","pad_x_mm","pad_y_mm","axis"],"properties":{
                "id":{"type":"string","description":"the container shape, by id or name"},
                "around":{"type":"array","items":{"type":"string"},"description":"ids or names of the layers it must contain"},
                "pad_x_mm":{"type":"number"},
                "pad_y_mm":{"type":"number"},
                "axis":{"type":"string","enum":["both","x","y"]}}}}),
        f(
            "path_boolean",
            "Merges several layers (path / shape / text) into one path. union joins, subtract cuts the others out of the first, intersect keeps the overlap, exclude removes it. The result replaces the first layer as an evenodd path (fills, strokes and effects kept); the others are deleted. Use union for shapes that form one body, e.g. the white outline band behind a title.",
            json!({"layer_ids":{"type":"array","items":{"type":"string"},"minItems":2},"op":{"type":"string","enum":["union","subtract","intersect","exclude"]}}),
            &["layer_ids", "op"],
        ),
        f(
            "outline_text",
            "Converts a text layer into an identical path layer (width ratio, slant and arc kept). Only for reshaping glyphs or merging with shapes through path_boolean; the wording can no longer be edited afterwards, so keep text as text otherwise.",
            json!({"layer_id":{"type":"string"}}),
            &["layer_id"],
        ),
        f(
            "trace_region",
            "Traces pixels close to color inside a region of the original into a vector path layer. For calligraphy, brush lettering, logotypes and flat-colored artwork that no font can reproduce. Text that a bundled font matches must be a text layer tuned with scale_x, letter spacing and strokes instead. Returns the mask iou and a render.",
            json!({"rect_px":rect_px.clone(),"color":{"type":"string","description":"#RRGGBB"},"tolerance":{"type":["integer","null"],"description":"per-channel tolerance, default 40"},"upsample":{"type":["integer","null"],"description":"upscale factor before tracing, default 8 (reduced automatically for large regions)"},"name":{"type":"string"},"group":{"type":["string","null"],"description":"parent group name (created when missing)"},"strategy":{"type":"string","enum":["text_traced","traced_vector","native_vector"]},"strategy_reason":{"type":"string"},"version":{"type":["integer","null"]}}),
            &["rect_px", "color", "name", "strategy", "strategy_reason"],
        ),
        f(
            "crop_original",
            "Crops a region of the original into an asset. transparent_color makes that color transparent. Returns the asset name and size.",
            json!({"rect_px":rect_px.clone(),"name":{"type":"string"},"version":{"type":["integer","null"]},"transparent_color":{"type":["string","null"]},"tolerance":{"type":["integer","null"]}}),
            &["rect_px", "name"],
        ),
        f(
            "cut_out",
            "Cuts a region of the original and removes its background into a PNG asset (1–3 s). Use it before generate_image for pictures, characters and photos in the original. Returns asset, size, place_mm (the original position in artboard mm), dpi (print resolution when placed at place_mm) and a preview. Place it with add_layer(kind=image, strategy=original_crop, strategy_reason='원본에서 따서 배경 제거') at place_mm. If dpi is below the target, do not enlarge it: redraw it with generate_image(transparent=true, references=[asset]).",
            json!({"rect_px":rect_px.clone(),"name":{"type":"string"},"version":{"type":["integer","null"]}}),
            &["rect_px", "name"],
        ),
        f(
            "generate_image",
            "Creates or edits a picture with the OpenAI image model. Write the prompt in English. references may hold asset names, 'original' or 'original:x,y,w,h' (a crop of the original) to redraw from. Saves the result as an asset and returns its name, size and a preview. Make one object per call; with transparent=true the result can be placed directly as a layer. Sizes are clamped to the API limits (multiples of 16, aspect 1:3–3:1, longest edge 3840 px).",
            json!({"prompt":{"type":"string"},"name":{"type":"string"},"width_px":{"type":["integer","null"]},"height_px":{"type":["integer","null"]},"quality":{"type":["string","null"],"enum":["low","medium","high","xhigh","max",null]},"transparent":{"type":["boolean","null"]},"references":{"type":["array","null"],"items":{"type":"string"}},"mask_rects_px":{"type":["array","null"],"items":rect_px.clone(),"description":"regions of the first reference to repaint (mask)"}}),
            &["prompt", "name"],
        ),
        f(
            "extend_background",
            "Outpaints a background asset on both sides until it matches the banner's aspect, blending each overlap and keeping the whole plate as a style reference. Call it once per plate, only when the banner is wider than the plate. Returns the new asset name and size.",
            json!({"asset":{"type":"string"},"prompt":{"type":["string","null"],"description":"optional English hint about the scenery to continue"},"height_px":{"type":["integer","null"],"description":"result height in px; leave null — the default already meets the background dpi requirement (never lower than that, max 2160)"}}),
            &["asset"],
        ),
        f(
            "list_fonts",
            "Lists the available fonts: family (the value for font_family), display_name (Korean name), category (고딕 / 명조 / 둥근 / 손글씨 / 디스플레이) and weights. Pick candidates here, then compare them with font_lineup.",
            json!({}),
            &[],
        ),
        f(
            "font_lineup",
            "Renders the same text in several candidate fonts, stacked and labelled, with the original crop on top, and measures how well each family matches the crop. Give rect_px as a tight crop of ONE line of the original text (one word of 2–6 large glyphs works best). Returns the picture plus `matches` — per family a score (overlap with the original's ink, 0–1) and the scale_x and letter_spacing_em that reproduce the original's proportions — and `reliable` (false for small, multi-colored or busy crops: then judge by the picture). Call it before creating a text layer whose font must match the original.",
            json!({"text":{"type":"string","description":"the text to render, usually the original wording"},"families":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":8,"description":"family names from list_fonts"},"weight":{"type":["integer","null"],"description":"font weight 100–900, default 700"},"rect_px":{"anyOf":[rect_px.clone(),{"type":"null"}],"description":"region of the original with this text, shown on top for comparison"},"version":{"type":["integer","null"]}}),
            &["text", "families"],
        ),
        f(
            "export_check",
            "Lists image layers whose print resolution is too low (current dpi and required dpi). The requirement is the target dpi, or half of it for strategy=background.",
            json!({"dpi":{"type":["number","null"]}}),
            &[],
        ),
        f(
            "set_memory",
            "Replaces the project memory (confirmed facts, written in Korean) as a whole.",
            json!({"text":{"type":"string"}}),
            &["text"],
        ),
        f(
            "append_memory",
            "Appends one Korean line to the project memory.",
            json!({"line":{"type":"string"}}),
            &["line"],
        ),
        f(
            "set_plan",
            "Saves the plan for a reconstruct or scratch turn. Call it once after looking at the original and before any edit (edit tools are refused until then); later rounds run on the standard model. steps are the ordered to-dos in Korean; image_quality sets the default generate_image quality (figure: people and pictures, background: the plate and extend_background).",
            json!({"steps":{"type":"array","items":{"type":"string"},"minItems":1},"image_quality":{"type":"object","properties":{"background":{"type":"string","enum":["low","medium","high"]},"figure":{"type":"string","enum":["low","medium","high"]}},"required":["background","figure"],"additionalProperties":false},"notes":{"type":["string","null"]}}),
            &["steps", "image_quality"],
        ),
        f(
            "set_fit_mode",
            "Saves how to fit the original into a banner of a different aspect. The ask_user answers '배경만 늘려 채우기' and '재배치' are saved automatically, so call this only when the answer was something else.",
            json!({"mode":{"type":"string","enum":["extend_background","relayout"],"description":"extend_background: keep positions and extend the plate; relayout: re-flow the layout for legibility"}}),
            &["mode"],
        ),
        f(
            "ask_user",
            "Asks the user and waits for the answers. Batch 1–3 questions in questions, all in Korean. options holds 2–4 choices (omit for free text only), allow_free_text permits typing, rect_px shows a region of the original next to the question (required when confirming text you read). The user may also choose '에이전트가 알아서'; the result then carries a note that the decision is yours.",
            json!({"questions":{"type":"array","minItems":1,"maxItems":3,"items":{"type":"object","properties":{"text":{"type":"string"},"options":{"type":["array","null"],"items":{"type":"string"}},"allow_free_text":{"type":["boolean","null"]},"rect_px":{"type":["array","null"],"items":{"type":"integer"}}},"required":["text"],"additionalProperties":false}}}),
            &["questions"],
        ),
    ]
}

pub fn label_for(name: &str, args: &Value) -> String {
    let s = |k: &str| args[k].as_str().unwrap_or("").to_string();
    match name {
        "get_document" => "문서 읽기".into(),
        "get_layer" => format!("레이어 보기 · {}", s("id")),
        "view_original" => "원본 보기".into(),
        "view_work" => "작업본 보기".into(),
        "font_lineup" => "글꼴 비교".into(),
        "compare" => "원본과 비교".into(),
        "analyze_original" => "원본 분석".into(),
        "measure" => "색·크기 재기".into(),
        "add_layer" => format!("레이어 추가 · {}", args["layer"]["name"].as_str().unwrap_or("")),
        "update_layer" => format!("레이어 고치기 · {}", s("id")),
        "delete_layer" => format!("레이어 지우기 · {}", s("id")),
        "move_layer" => format!("레이어 순서 · {}", s("id")),
        "arrange_row" => format!("줄 맞추기 · {}", args["items"][0][0].as_str().unwrap_or("")),
        "fit_box" => format!("상자 맞추기 · {}", s("id")),
        "path_boolean" => format!(
            "패스 합치기 · {}",
            match s("op").as_str() {
                "subtract" => "빼기",
                "intersect" => "겹친 곳만",
                "exclude" => "겹친 곳 빼기",
                _ => "합치기",
            }
        ),
        "outline_text" => format!("글자 윤곽선 · {}", s("layer_id")),
        "trace_region" => format!("모양 따기 · {}", s("name")),
        "crop_original" => format!("원본 조각 자르기 · {}", s("name")),
        "cut_out" => format!("누끼 따기 · {}", s("name")),
        "generate_image" => format!("그림 만들기 · {}", s("name")),
        "extend_background" => "배경 이어 그리기".into(),
        "list_fonts" => "글꼴 목록".into(),
        "export_check" => "출력 해상도 점검".into(),
        "set_memory" | "append_memory" => "메모리 기록".into(),
        "ask_user" => "사용자에게 묻기".into(),
        "set_fit_mode" => "배치 방식 저장".into(),
        "set_plan" => "계획 세우기".into(),
        _ => name.to_string(),
    }
}

pub async fn execute(ctx: &ToolCtx, name: &str, args: Value) -> anyhow::Result<ToolResult> {
    guard(&ctx.state, name)?;
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

fn execute_sync(ctx: &ToolCtx, name: &str, args: Value) -> anyhow::Result<ToolResult> {
    if READ_TOOLS.contains(&name) {
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
        "get_layer" => {
            let id = arg_str(&args, "id")?;
            let h = img::lock(&ctx.handle);
            let l = h.document.layer(&id).ok_or_else(|| anyhow!("레이어 {id} 없음"))?;
            Ok(ToolResult::json(serde_json::to_value(l)?))
        }
        "add_layer" => {
            let artboard = {
                let h = img::lock(&ctx.handle);
                (h.document.artboard.width_mm, h.document.artboard.height_mm)
            };
            let mut layer = layer_args::parse_layer(&args["layer"], artboard)?;
            layer.name = tidy_name(&layer.name);
            let index = args["index"].as_u64().map(|v| v as usize);
            let touched = {
                let mut h = img::lock(&ctx.handle);
                ensure_running(ctx)?;
                insert_layer(&mut h, layer, index, &ctx.turn)?
            };
            ctx.state.edits.fetch_add(1, Ordering::SeqCst);
            edit_result(ctx, &touched, &format!("레이어 {touched} 추가"))
        }
        "update_layer" => {
            let id = arg_str(&args, "id")?;
            let mut patch = layer_args::patch_from_args(&args["patch"]);
            if let Some(name) = patch.get_mut("name")
                && let Some(s) = name.as_str()
            {
                *name = json!(tidy_name(s));
            }
            if patch.as_object().is_none_or(|m| m.is_empty()) {
                bail!("patch에 바꿀 필드가 없습니다. 바꿀 필드만 값을 넣고 나머지는 null로 보내세요.");
            }
            if patch.get("strategy").is_some() {
                ctx.state.promoted.fetch_add(1, Ordering::SeqCst);
            }
            {
                let mut h = img::lock(&ctx.handle);
                ensure_running(ctx)?;
                h.apply_ops(
                    vec![Op::UpdateLayer {
                        id: id.clone(),
                        patch,
                    }],
                    Actor::Agent,
                    Some(ctx.turn.clone()),
                )?;
            }
            ctx.state.edits.fetch_add(1, Ordering::SeqCst);
            edit_result(ctx, &id, &format!("레이어 {id} 수정"))
        }
        "path_boolean" | "outline_text" => {
            let doc = img::lock(&ctx.handle).document.clone();
            let (ops, id, note) = {
                if name == "outline_text" {
                    let id = arg_str(&args, "layer_id")?;
                    let ops = boolean::outline_text(&doc, &ctx.fonts, &id)?;
                    (ops, id.clone(), format!("{id} 글자 윤곽선"))
                } else {
                    let ids: Vec<LayerId> = serde_json::from_value(args["layer_ids"].clone())
                        .map_err(|_| anyhow!("layer_ids는 레이어 id 배열이어야 합니다"))?;
                    let op: BooleanOp = serde_json::from_value(args["op"].clone())
                        .map_err(|_| anyhow!("op는 union·subtract·intersect·exclude 중 하나입니다"))?;
                    let first = ids
                        .first()
                        .cloned()
                        .ok_or_else(|| anyhow!("layer_ids가 비었습니다"))?;
                    let ops = boolean::path_boolean(&doc, &ctx.fonts, &ids, op)?;
                    (ops, first, format!("{} 패스 합치기", ids.join("+")))
                }
            };
            {
                let mut h = img::lock(&ctx.handle);
                ensure_running(ctx)?;
                h.apply_ops(ops, Actor::Agent, Some(ctx.turn.clone()))?;
            }
            ctx.state.edits.fetch_add(1, Ordering::SeqCst);
            edit_result(ctx, &id, &note)
        }
        "delete_layer" => {
            let id = arg_str(&args, "id")?;
            let bounds = {
                let mut h = img::lock(&ctx.handle);
                let b = h.document.layer(&id).map(|l| layer_bounds_mm(&h.document, l));
                ensure_running(ctx)?;
                h.apply_ops(
                    vec![Op::DeleteLayer { id: id.clone() }],
                    Actor::Agent,
                    Some(ctx.turn.clone()),
                )?;
                b
            };
            ctx.state.edits.fetch_add(1, Ordering::SeqCst);
            match bounds {
                Some(b) => region_result(ctx, b, &format!("레이어 {id} 삭제")),
                None => Ok(ToolResult::text(format!("레이어 {id} 삭제"))),
            }
        }
        "move_layer" => {
            let id = arg_str(&args, "id")?;
            let parent = args["parent"].as_str().map(|s| s.to_string());
            let index = args["index"].as_u64().unwrap_or(0) as usize;
            {
                let mut h = img::lock(&ctx.handle);
                ensure_running(ctx)?;
                // A group name works like in add_layer: existing group by id or name, else created.
                let parent = match parent {
                    Some(p) if h.document.layer(&p).is_none() => Some(ensure_group(&mut h, &p, &ctx.turn)?),
                    other => other,
                };
                h.apply_ops(
                    vec![Op::MoveLayer {
                        id: id.clone(),
                        parent,
                        index,
                    }],
                    Actor::Agent,
                    Some(ctx.turn.clone()),
                )?;
            }
            ctx.state.edits.fetch_add(1, Ordering::SeqCst);
            Ok(ToolResult::text(format!("레이어 {id} 이동")))
        }
        "arrange_row" => arrange_row(ctx, &args),
        "fit_box" => fit_box(ctx, &args),
        "trace_region" => trace_region(ctx, &args),
        "list_fonts" => {
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
        "export_check" => {
            let h = img::lock(&ctx.handle);
            let dpi = args["dpi"].as_f64().unwrap_or(h.project.target_dpi);
            Ok(ToolResult::json(
                json!({"dpi": dpi, "warnings": export_check(&h.document, dpi)}),
            ))
        }
        "set_memory" => {
            let text = arg_str(&args, "text")?;
            img::lock(&ctx.handle).set_memory(&text)?;
            ctx.state.memory_dirty.store(true, Ordering::SeqCst);
            Ok(ToolResult::text("메모리 저장"))
        }
        "append_memory" => {
            let line = arg_str(&args, "line")?;
            let h = img::lock(&ctx.handle);
            let mut m = h.memory().unwrap_or_default();
            if !m.ends_with('\n') && !m.is_empty() {
                m.push('\n');
            }
            m.push_str(&format!("- {line}\n"));
            h.set_memory(&m)?;
            ctx.state.memory_dirty.store(true, Ordering::SeqCst);
            Ok(ToolResult::text("메모리에 추가"))
        }
        "set_fit_mode" => {
            let mode: FitMode = serde_json::from_value(args["mode"].clone())
                .map_err(|_| anyhow!("mode는 extend_background 또는 relayout이어야 합니다"))?;
            img::lock(&ctx.handle).set_fit_mode(mode)?;
            ctx.state.fit_pending.store(false, Ordering::SeqCst);
            Ok(ToolResult::text(prompt::fit_mode_instruction(mode)))
        }
        "set_plan" => {
            let plan: Plan =
                serde_json::from_value(args.clone()).map_err(|e| anyhow!("set_plan 인자 오류: {e}"))?;
            if plan.steps.is_empty() {
                bail!("steps에 할 일을 하나 이상 넣으세요");
            }
            {
                let h = img::lock(&ctx.handle);
                let mut st = super::load_agent_state(&h);
                st["plan"] = json!({"turn": ctx.turn, "plan": plan});
                super::save_agent_state(&h, &st);
            }
            let text = format!(
                "계획을 저장했습니다({}단계). 다음 라운드부터 실행 모델로 진행합니다. 그림 기본 품질 {}, 배경 기본 품질 {}.",
                plan.steps.len(),
                plan.image_quality.figure.as_str(),
                plan.image_quality.background.as_str()
            );
            *ctx.state.plan.lock().unwrap_or_else(|e| e.into_inner()) = Some(plan);
            ctx.state.plan_pending.store(false, Ordering::SeqCst);
            Ok(ToolResult::text(text))
        }
        _ => bail!("알 수 없는 도구 {name}"),
    }
}

fn guard(state: &TurnState, name: &str) -> anyhow::Result<()> {
    if !EDIT_TOOLS.contains(&name) {
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

fn read_version(ctx: &ToolCtx, name: &str, args: &Value) -> u64 {
    match name {
        "get_document" | "view_work" | "compare" => img::lock(&ctx.handle).document.rev,
        _ => version_of(ctx, args).map(u64::from).unwrap_or(0),
    }
}

fn read_tool(ctx: &ToolCtx, name: &str, args: &Value) -> anyhow::Result<ToolResult> {
    match name {
        "get_document" => Ok(ToolResult::json(document_summary(
            &img::lock(&ctx.handle).document,
        ))),
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

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

fn region_words(rect: [f64; 4], width: f64, height: f64) -> String {
    let (width, height) = (width.max(1.0), height.max(1.0));
    let ratio = (rect[2] * rect[3]) / (width * height);
    if ratio >= 0.9 {
        return "거의 전체".into();
    }
    let cx = (rect[0] + rect[2] / 2.0) / width;
    let cy = (rect[1] + rect[3] / 2.0) / height;
    let horizontal = if cx < 1.0 / 3.0 {
        "왼쪽"
    } else if cx > 2.0 / 3.0 {
        "오른쪽"
    } else {
        ""
    };
    let vertical = if cy < 1.0 / 3.0 {
        "위"
    } else if cy > 2.0 / 3.0 {
        "아래"
    } else {
        ""
    };
    let place = match (horizontal, vertical) {
        ("", "") => "가운데".to_string(),
        (h, "") => h.to_string(),
        ("", v) => format!("{v}쪽"),
        (h, v) => format!("{h} {v}"),
    };
    let share = (1.0 / ratio.max(0.001)).round() as u32;
    format!("{place} 1/{share} 영역")
}

fn rect4(args: &Value, key: &str) -> Option<[f64; 4]> {
    let a = args[key].as_array()?;
    if a.len() != 4 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?, a[3].as_f64()?])
}

pub fn detail_for(name: &str, args: &Value, doc: &Document, facts: Option<&OriginalFacts>) -> Option<String> {
    let layer = |key: &str| {
        args[key].as_str().map(|id| {
            doc.layer(id)
                .map(|l| l.name.clone())
                .unwrap_or_else(|| id.to_string())
        })
    };
    let original_region = || -> String {
        match (rect4(args, "rect_px"), facts) {
            (Some(r), Some(f)) => region_words(r, f.width_px as f64, f.height_px as f64),
            (Some(_), None) => "일부 영역".into(),
            (None, _) => "전체".into(),
        }
    };
    let work_region = || -> String {
        match rect4(args, "rect_mm") {
            Some(r) => region_words(r, doc.artboard.width_mm, doc.artboard.height_mm),
            None => "전체".into(),
        }
    };
    let max_px = args["max_px"].as_u64().unwrap_or(1536);
    let with_color = |base: String| match args["color"].as_str() {
        Some(c) => format!("{base} · {c}"),
        None => base,
    };
    match name {
        "view_original" => Some(format!("{}, {max_px}px", original_region())),
        "view_work" => match args["layer_ids"].as_array() {
            Some(ids) => {
                let names: Vec<String> = ids
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|id| {
                        doc.layer(id)
                            .map(|l| l.name.clone())
                            .unwrap_or_else(|| id.to_string())
                    })
                    .collect();
                Some(format!("레이어 {}", clip(&names.join(", "), 40)))
            }
            None => Some(format!("{}, {max_px}px", work_region())),
        },
        "compare" => Some(work_region()),
        "analyze_original" => args["version"].as_u64().map(|v| format!("원본 v{v}")),
        "measure" | "trace_region" => Some(with_color(original_region())),
        "crop_original" | "cut_out" => Some(original_region()),
        "get_layer" | "delete_layer" => layer("id").map(|n| format!("'{n}'")),
        "outline_text" => layer("layer_id").map(|n| format!("'{n}'")),
        "path_boolean" => args["layer_ids"].as_array().map(|ids| {
            let names: Vec<String> = ids
                .iter()
                .filter_map(Value::as_str)
                .map(|id| doc.layer(id).map_or_else(|| id.to_string(), |l| l.name.clone()))
                .collect();
            clip(&names.join(" + "), 40)
        }),
        "update_layer" => layer("id").map(|n| {
            let paths: Vec<String> =
                hoist_core::ops::patch_paths(&layer_args::patch_from_args(&args["patch"]))
                    .into_iter()
                    .filter(|p| p != "pinned")
                    .take(3)
                    .collect();
            format!("'{n}' · {}", paths.join(", "))
        }),
        "move_layer" => layer("id").map(|n| format!("'{n}' → {}번째", args["index"].as_u64().unwrap_or(0))),
        "add_layer" => {
            let l = &args["layer"];
            let label = clip(l["name"].as_str().unwrap_or(""), 20);
            Some(match l["kind"]["type"].as_str() {
                Some("text") => format!(
                    "글자 '{}'",
                    clip(l["kind"]["text"].as_str().unwrap_or(&label), 20)
                ),
                Some("image") => format!("그림 '{label}'"),
                Some("shape") => format!("도형 '{label}'"),
                Some("path") => format!("모양 '{label}'"),
                Some("group") => format!("묶음 '{label}'"),
                _ => format!("'{label}'"),
            })
        }
        "generate_image" => args["prompt"].as_str().map(|p| format!("'{}'", clip(p, 30))),
        "extend_background" => args["asset"].as_str().map(|a| format!("'{a}'")),
        "set_memory" => args["text"].as_str().map(|t| clip(t, 30)),
        "append_memory" => args["line"].as_str().map(|t| clip(t, 30)),
        "ask_user" => {
            let texts: Vec<&str> = match args["questions"].as_array() {
                Some(qs) => qs.iter().filter_map(|q| q["text"].as_str()).collect(),
                None => args["question"].as_str().into_iter().collect(),
            };
            texts.first().map(|first| match texts.len() {
                1 => clip(first, 30),
                n => format!("{} 외 {}개", clip(first, 24), n - 1),
            })
        }
        "set_plan" => args["steps"].as_array().map(|steps| {
            format!(
                "{}단계 · 그림 {} · 배경 {}",
                steps.len(),
                args["image_quality"]["figure"].as_str().unwrap_or("?"),
                args["image_quality"]["background"].as_str().unwrap_or("?")
            )
        }),
        "export_check" => args["dpi"].as_f64().map(|d| format!("{d}dpi")),
        _ => None,
    }
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

fn facts(ctx: &ToolCtx) -> anyhow::Result<&OriginalFacts> {
    ctx.facts
        .as_ref()
        .ok_or_else(|| anyhow!("이 프로젝트에는 원본이 없습니다"))
}

fn version_of(ctx: &ToolCtx, args: &Value) -> anyhow::Result<u32> {
    match args["version"].as_u64() {
        Some(v) => Ok(v as u32),
        None => Ok(facts(ctx)?.version),
    }
}

pub fn document_summary(doc: &Document) -> Value {
    let layers: Vec<Value> = doc
        .layers
        .iter()
        .map(|l| {
            let (kind, extra) = match &l.kind {
                LayerKind::Group => ("group", Value::Null),
                LayerKind::Text(t) => (
                    "text",
                    json!({"text": t.text, "font": t.font_family, "size_mm": t.font_size_mm}),
                ),
                LayerKind::Path(_) => ("path", Value::Null),
                LayerKind::Shape(s) => ("shape", serde_json::to_value(s).unwrap_or(Value::Null)),
                LayerKind::Image(i) => (
                    "image",
                    json!({"asset": i.asset, "px": [i.natural_w_px, i.natural_h_px]}),
                ),
            };
            json!({"id": l.id, "name": l.name, "parent": l.parent, "kind": kind, "extra": extra,
                "x_mm": r1(l.x_mm), "y_mm": r1(l.y_mm), "w_mm": r1(l.w_mm), "h_mm": r1(l.h_mm),
                "visible": l.visible, "strategy": l.strategy, "pinned": l.pinned,
                "fills": l.fills.len(), "strokes": l.strokes.len(), "effects": l.effects.len()})
        })
        .collect();
    json!({"rev": doc.rev, "artboard": doc.artboard, "layers": layers})
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub fn tidy_name(name: &str) -> String {
    name.replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Tool tasks already running when the user stops would otherwise still edit the document behind a "failed" action.
fn ensure_running(ctx: &ToolCtx) -> anyhow::Result<()> {
    if ctx.run.stop.load(Ordering::SeqCst) {
        bail!("사용자가 작업을 중단해 적용하지 않았어요");
    }
    Ok(())
}

fn insert_layer(
    h: &mut ProjectHandle,
    mut layer: Layer,
    index: Option<usize>,
    turn: &str,
) -> anyhow::Result<LayerId> {
    if let Some(p) = layer.parent.clone()
        && h.document.layer(&p).is_none()
    {
        layer.parent = Some(ensure_group(h, &p, turn)?);
    }
    if layer.id.is_empty() || h.document.layer(&layer.id).is_some() {
        layer.id = h.document.next_id();
    }
    let id = layer.id.clone();
    h.apply_ops(
        vec![Op::AddLayer {
            layer: Box::new(layer),
            index,
        }],
        Actor::Agent,
        Some(turn.to_string()),
    )?;
    Ok(id)
}

fn ensure_group(h: &mut ProjectHandle, name: &str, turn: &str) -> anyhow::Result<LayerId> {
    let name = tidy_name(name);
    let name = name.as_str();
    if let Some(g) = h
        .document
        .layers
        .iter()
        .find(|l| l.parent.is_none() && matches!(l.kind, LayerKind::Group) && tidy_name(&l.name) == name)
    {
        return Ok(g.id.clone());
    }
    let id = h.document.next_id();
    let layer = Layer {
        id: id.clone(),
        name: name.to_string(),
        parent: None,
        kind: LayerKind::Group,
        x_mm: 0.0,
        y_mm: 0.0,
        w_mm: h.document.artboard.width_mm,
        h_mm: h.document.artboard.height_mm,
        rotation_deg: 0.0,
        opacity: 1.0,
        visible: true,
        locked: false,
        fills: vec![],
        strokes: vec![],
        effects: vec![],
        strategy: Strategy::NativeVector,
        strategy_reason: "그룹".into(),
        pinned: vec![],
        source: None,
    };
    let index = match name {
        "배경" => Some(0),
        _ => None,
    };
    h.apply_ops(
        vec![Op::AddLayer {
            layer: Box::new(layer),
            index,
        }],
        Actor::Agent,
        Some(turn.to_string()),
    )?;
    Ok(id)
}

fn edit_result(ctx: &ToolCtx, layer_id: &str, note: &str) -> anyhow::Result<ToolResult> {
    let (bounds, layer) = {
        let h = img::lock(&ctx.handle);
        let l = h
            .document
            .layer(layer_id)
            .ok_or_else(|| anyhow!("레이어 {layer_id} 없음"))?;
        (layer_bounds_mm(&h.document, l), l.clone())
    };
    let res = region_result(ctx, bounds, note)?;
    if !matches!(layer.kind, LayerKind::Text(_)) {
        return Ok(res);
    }
    let (ink, lines) = ink_mm(&ctx.fonts, &layer);
    Ok(with_fields(res, json!({"ink_mm": ink.map(r1), "lines": lines})))
}

fn with_fields(mut res: ToolResult, extra: Value) -> ToolResult {
    let mut v: Value = serde_json::from_str(&res.text).unwrap_or_else(|_| json!({}));
    if let (Some(m), Value::Object(extra)) = (v.as_object_mut(), extra) {
        m.extend(extra);
    }
    res.text = v.to_string();
    res
}

/// The glyph box for text layers, the layer box for everything else, and the laid-out line count of text.
pub fn ink_mm(fonts: &Fonts, l: &Layer) -> ([f64; 4], usize) {
    let own = [l.x_mm, l.y_mm, l.w_mm, l.h_mm];
    let LayerKind::Text(t) = &l.kind else {
        return (own, 0);
    };
    match fonts.layout_to_path(t, l.w_mm, l.h_mm) {
        Ok(run) if run.bbox_mm[2] > 0.0 && run.bbox_mm[3] > 0.0 => (
            [
                l.x_mm + run.bbox_mm[0],
                l.y_mm + run.bbox_mm[1],
                run.bbox_mm[2],
                run.bbox_mm[3],
            ],
            run.lines,
        ),
        Ok(run) => (own, run.lines),
        Err(_) => (own, 0),
    }
}

pub fn union_mm(boxes: impl IntoIterator<Item = [f64; 4]>) -> Option<[f64; 4]> {
    boxes
        .into_iter()
        .map(|b| [b[0], b[1], b[0] + b[2], b[1] + b[3]])
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
        .map(|u| [u[0], u[1], u[2] - u[0], u[3] - u[1]])
}

fn find_layer<'a>(doc: &'a Document, r: &str) -> anyhow::Result<&'a Layer> {
    doc.layer(r)
        .or_else(|| doc.layers.iter().find(|l| l.name == r))
        .or_else(|| doc.layers.iter().find(|l| l.name == tidy_name(r)))
        .ok_or_else(|| anyhow!("layer '{r}' not found: give a layer id or its exact name"))
}

fn layer_refs(v: &Value, key: &str) -> anyhow::Result<Vec<String>> {
    let refs: Vec<String> = serde_json::from_value(v.clone())
        .map_err(|_| anyhow!("{key} must be an array of layer ids or names"))?;
    if refs.is_empty() {
        bail!("{key} must name at least one layer");
    }
    Ok(refs)
}

fn arrange_row(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let items = args["items"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| anyhow!("items must hold at least one item"))?
        .iter()
        .map(|item| layer_refs(item, "each item"))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let number = |key: &str| {
        args[key]
            .as_f64()
            .ok_or_else(|| anyhow!("{key} must be a number"))
    };
    let (x0, x1) = (number("x0_mm")?, number("x1_mm")?);
    let justify = args["justify"].as_str().unwrap_or("");
    if !["center", "start", "end", "space_between"].contains(&justify) {
        bail!("justify must be center, start, end or space_between");
    }
    let n = items.len();
    let mut moved = Vec::new();
    let result = {
        let mut h = img::lock(&ctx.handle);
        ensure_running(ctx)?;
        let doc = &h.document;
        let mut rows = Vec::new();
        for refs in &items {
            let layers = refs
                .iter()
                .map(|r| find_layer(doc, r))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let ink = union_mm(layers.iter().map(|l| ink_mm(&ctx.fonts, l).0)).unwrap_or_default();
            rows.push((layers, ink));
        }
        let range = x1 - x0;
        let content: f64 = rows.iter().map(|(_, b)| b[2]).sum();
        let spread = justify == "space_between" && n > 1;
        let gap = if spread {
            (range - content) / (n - 1) as f64
        } else {
            args["gap_mm"].as_f64().unwrap_or(0.0)
        };
        let total = content + gap * (n - 1) as f64;
        let start = match justify {
            "start" => x0,
            "end" => x1 - total,
            _ if spread => x0,
            _ => x0 + (range - total) / 2.0,
        };
        let mut ops = Vec::new();
        let mut out = Vec::new();
        let mut centers = Vec::new();
        let mut cursor = start;
        for (i, (layers, b)) in rows.iter().enumerate() {
            let dx = cursor - b[0];
            for l in layers {
                ops.push(Op::UpdateLayer {
                    id: l.id.clone(),
                    patch: json!({"x_mm": l.x_mm + dx}),
                });
                moved.push(l.id.clone());
            }
            let names: Vec<&str> = layers.iter().map(|l| l.name.as_str()).collect();
            let ink = [b[0] + dx, b[1], b[2], b[3]].map(r1);
            out.push(json!({"layers": names, "ink_mm": ink}));
            cursor += b[2];
            if i + 1 < n {
                centers.push(r1(cursor + gap / 2.0));
            }
            cursor += gap;
        }
        let mut result = json!({"items": out, "content_mm": r1(content), "gap_mm": r1(gap), "row_mm": [r1(start), r1(start + total)], "gap_centers_mm": centers, "free_mm": r1(range - total)});
        if total > range + 0.5 || gap < 0.0 {
            result["overflow"] = json!(format!(
                "content is {} mm wider than the range: reduce the font size or re-break lines, then call again",
                r1((content - range).max(total - range))
            ));
        }
        h.apply_ops(ops, Actor::Agent, Some(ctx.turn.clone()))?;
        result
    };
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    let bounds = {
        let h = img::lock(&ctx.handle);
        union_mm(
            moved
                .iter()
                .filter_map(|id| h.document.layer(id))
                .map(|l| layer_bounds_mm(&h.document, l)),
        )
        .unwrap_or_default()
    };
    let res = region_result(ctx, bounds, &format!("{n}개 항목 줄 맞춤"))?;
    Ok(with_fields(res, result))
}

fn fit_box(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let target = arg_str(args, "id")?;
    let around = layer_refs(&args["around"], "around")?;
    let (pad_x, pad_y) = (
        args["pad_x_mm"].as_f64().unwrap_or(0.0),
        args["pad_y_mm"].as_f64().unwrap_or(0.0),
    );
    let axis = args["axis"].as_str().unwrap_or("both");
    if !["both", "x", "y"].contains(&axis) {
        bail!("axis must be both, x or y");
    }
    let (id, content, placed) = {
        let mut h = img::lock(&ctx.handle);
        ensure_running(ctx)?;
        let doc = &h.document;
        let shape = find_layer(doc, &target)?;
        if !matches!(shape.kind, LayerKind::Shape(_)) {
            bail!(
                "'{}' is not a shape layer: fit_box only resizes shapes",
                shape.name
            );
        }
        let id = shape.id.clone();
        let layers = around
            .iter()
            .map(|r| find_layer(doc, r))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let b = union_mm(layers.iter().map(|l| ink_mm(&ctx.fonts, l).0)).unwrap_or_default();
        let mut patch = serde_json::Map::new();
        if axis != "y" {
            patch.insert("x_mm".into(), json!(b[0] - pad_x));
            patch.insert("w_mm".into(), json!(b[2] + 2.0 * pad_x));
        }
        if axis != "x" {
            patch.insert("y_mm".into(), json!(b[1] - pad_y));
            patch.insert("h_mm".into(), json!(b[3] + 2.0 * pad_y));
        }
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: id.clone(),
                patch: Value::Object(patch),
            }],
            Actor::Agent,
            Some(ctx.turn.clone()),
        )?;
        let l = h.document.layer(&id).ok_or_else(|| anyhow!("레이어 {id} 없음"))?;
        let placed = [l.x_mm, l.y_mm, l.w_mm, l.h_mm];
        (id, b, placed)
    };
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    let res = edit_result(ctx, &id, &format!("레이어 {id} 상자 맞춤"))?;
    Ok(with_fields(
        res,
        json!({"layer_id": id, "box_mm": placed.map(r1), "content_mm": content.map(r1)}),
    ))
}

fn region_result(ctx: &ToolCtx, bounds: [f64; 4], note: &str) -> anyhow::Result<ToolResult> {
    let pad_w = bounds[2] * 0.15 + 10.0;
    let pad_h = bounds[3] * 0.15 + 10.0;
    let (doc, assets, original_path, rect) = {
        let h = img::lock(&ctx.handle);
        let rect = img::clamp_rect_mm(
            &h,
            [
                bounds[0] - pad_w,
                bounds[1] - pad_h,
                bounds[2] + pad_w * 2.0,
                bounds[3] + pad_h * 2.0,
            ],
        );
        let original_path = ctx.facts.as_ref().map(|f| h.original_path(f.version));
        (h.document.clone(), h.assets_dir(), original_path, rect)
    };
    match (ctx.facts.as_ref(), original_path) {
        (Some(f), Some(path)) => {
            let original = cimg::load(&path)?;
            let (res, png) = img::compare_region(&doc, &assets, &original, &ctx.fonts, f, rect, 900)?;
            Ok(ToolResult { cached: false,
                text: json!({"note": note, "region_mm": rect, "similarity": r3(res.similarity), "overlap": r3(res.overlap), "edge_similarity": r3(res.edge_similarity), "image": "왼쪽 원본 · 오른쪽 작업본"}).to_string(),
                images: vec![png],
            })
        }
        _ => {
            let im = img::work_region(&doc, &assets, &ctx.fonts, rect, 900)?;
            Ok(ToolResult {
                cached: false,
                text: json!({"note": note, "region_mm": rect}).to_string(),
                images: vec![cimg::to_png_bytes(&im)?],
            })
        }
    }
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn view_original(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let version = version_of(ctx, args)?;
    let max_px = args["max_px"].as_u64().unwrap_or(1536).clamp(256, 3000) as u32;
    let h = img::lock(&ctx.handle);
    let orig = img::original_image(&h, version)?;
    let (img_out, rect) = match arg_rect_px_opt(args, "rect_px")? {
        Some(r) => (cimg::crop(&orig, r), Some(r)),
        None => (orig.clone(), None),
    };
    let small = cimg::resize_max(&img_out, max_px);
    Ok(ToolResult { cached: false,
        text: json!({"version": version, "width_px": orig.width(), "height_px": orig.height(), "shown_rect_px": rect, "shown_px": [small.width(), small.height()]}).to_string(),
        images: vec![cimg::to_png_bytes(&small)?],
    })
}

fn font_lineup(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let text = arg_str(args, "text")?;
    let families: Vec<String> = args["families"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if families.is_empty() || families.len() > 8 {
        bail!("families must hold 1–8 family names from list_fonts");
    }
    let missing: Vec<&str> = families
        .iter()
        .filter(|f| !ctx.fonts.has_family(f))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        bail!(
            "unknown families: {} — use family names from list_fonts",
            missing.join(", ")
        );
    }
    let weight = args["weight"].as_u64().unwrap_or(700).clamp(100, 900) as u16;
    let lineup = hoist_core::render::font_lineup(&ctx.fonts, &text, &families, weight, 20.0)?;
    let (image, body) = match arg_rect_px_opt(args, "rect_px")? {
        Some(rect) => {
            let version = version_of(ctx, args)?;
            let original = img::original_image(&img::lock(&ctx.handle), version)?;
            let crop = cimg::crop(&original, rect);
            let crop_h = (f64::from(crop.height()) * f64::from(lineup.width())
                / f64::from(crop.width().max(1)))
            .round()
            .max(1.0) as u32;
            let fit =
                hoist_core::fontmatch::fit_fonts(&ctx.fonts, &original, rect, &text, &families, weight)?;
            (
                stack_vertical(&cimg::resize_to(&crop, lineup.width(), crop_h), &lineup, 24),
                json!({"families": families, "weight": weight, "matches": fit.matches, "reliable": fit.reliable, "note": "Top: the original crop. Below: each candidate labelled with its family. matches lists, per family, the overlap score with the original's ink and the scale_x / letter_spacing_em that reproduce its proportions (fitted on the first line of the crop). When reliable is true use the top score; ties within 0.02 are broken by stroke ends in the picture. When reliable is false judge by the picture and still use the fitted scale_x of the family you pick."}),
            )
        }
        None => (
            lineup,
            json!({"families": families, "weight": weight, "note": "Top: the original crop (when rect_px was given). Below: each candidate labelled with its family. Compare the skeletons (ㅇ roundness, stroke ends, ㄹ/ㅁ proportions, baseline), pick the closest, then tune font_weight, scale_x and letter_spacing_mm on the text layer."}),
        ),
    };
    Ok(ToolResult {
        cached: false,
        text: body.to_string(),
        images: vec![cimg::to_png_bytes(&cimg::resize_max(&image, 1536))?],
    })
}

fn stack_vertical(top: &RgbaImage, bottom: &RgbaImage, gap: u32) -> RgbaImage {
    let w = top.width().max(bottom.width());
    let mut out = RgbaImage::from_pixel(
        w,
        top.height() + gap + bottom.height(),
        image::Rgba([255, 255, 255, 255]),
    );
    for (src, dy) in [(top, 0), (bottom, top.height() + gap)] {
        for (x, y, p) in src.enumerate_pixels() {
            out.put_pixel(x, y + dy, *p);
        }
    }
    out
}

fn arg_rect_px_opt(args: &Value, key: &str) -> anyhow::Result<Option<[u32; 4]>> {
    if args[key].is_null() {
        return Ok(None);
    }
    Ok(Some(arg_rect_px(args, key)?))
}

fn view_work(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let max_px = args["max_px"].as_u64().unwrap_or(1536).clamp(256, 3000) as u32;
    let h = img::lock(&ctx.handle);
    let image = if let Some(ids) = args["layer_ids"].as_array() {
        let ids: Vec<String> = ids
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let mut doc = h.document.clone();
        for l in doc.layers.iter_mut() {
            let keep =
                ids.iter().any(|i| i == &l.id) || l.parent.as_ref().map(|p| ids.contains(p)).unwrap_or(false);
            if !keep && !matches!(l.kind, LayerKind::Group) {
                l.visible = false;
            }
        }
        let scale = img::work_scale(&h, max_px);
        hoist_core::render::render_document(
            &doc,
            &h.assets_dir(),
            &ctx.fonts,
            &hoist_core::render::RenderOptions {
                scale_px_per_mm: scale,
                background: false,
            },
        )?
    } else if let Some(r) = arg_rect_mm(args, "rect_mm") {
        img::work_region(
            &h.document,
            &h.assets_dir(),
            &ctx.fonts,
            img::clamp_rect_mm(&h, r),
            max_px,
        )?
    } else {
        img::work_image(&h, &ctx.fonts, max_px)?
    };
    Ok(ToolResult {
        cached: false,
        text: json!({"shown_px": [image.width(), image.height()], "rev": h.document.rev}).to_string(),
        images: vec![cimg::to_png_bytes(&image)?],
    })
}

fn compare_tool(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let f = facts(ctx)?;
    let h = img::lock(&ctx.handle);
    if let Some(r) = arg_rect_mm(args, "rect_mm") {
        let rect = img::clamp_rect_mm(&h, r);
        let original = img::original_image(&h, f.version)?;
        let (res, png) =
            img::compare_region(&h.document, &h.assets_dir(), &original, &ctx.fonts, f, rect, 1200)?;
        return Ok(ToolResult {
            cached: false,
            text: json!({"region_mm": rect, "result": res, "image": "왼쪽 원본 · 오른쪽 작업본"}).to_string(),
            images: vec![png],
        });
    }
    let full = img::compare_full(&h, &ctx.fonts, f, 1200)?;
    Ok(ToolResult {
        cached: false,
        text: json!({"result": full.result, "image": "왼쪽 원본 · 오른쪽 작업본"}).to_string(),
        images: vec![full.side_by_side_png],
    })
}

fn analyze_original(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let version = version_of(ctx, args)?;
    let f = facts(ctx)?;
    let h = img::lock(&ctx.handle);
    let cache = h.dir.join("analysis").join(format!("v{version}.json"));
    let cached: Option<Value> = std::fs::read(&cache)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let analysis = match cached {
        Some(v) => v,
        None => {
            let orig = img::original_image(&h, version)?;
            let small = cimg::resize_max(&orig, 1200);
            let scale = orig.width() as f64 / small.width() as f64;
            let pal = analyze::palette(&small, 10);
            let regions: Vec<Value> = analyze::segment_regions(&small)
                .into_iter()
                .map(|r| {
                    let b = r.bbox_px;
                    json!({"bbox_px": [(b[0] as f64 * scale).round(), (b[1] as f64 * scale).round(), (b[2] as f64 * scale).round(), (b[3] as f64 * scale).round()], "color": r.color, "kind": r.kind_hint, "area_px": (r.area_px as f64 * scale * scale).round()})
                })
                .collect();
            let v = json!({"width_px": orig.width(), "height_px": orig.height(), "palette": pal.colors, "regions": regions});
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&cache, serde_json::to_vec(&v)?);
            v
        }
    };
    Ok(ToolResult::json(
        json!({"version": version, "width_px": analysis["width_px"], "height_px": analysis["height_px"], "mm_per_px": f.mm_per_px, "offset_mm": [f.offset_x_mm, f.offset_y_mm], "palette": analysis["palette"], "regions": analysis["regions"]}),
    ))
}

fn measure(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let version = version_of(ctx, args)?;
    let rect = arg_rect_px(args, "rect_px")?;
    let h = img::lock(&ctx.handle);
    let orig = img::original_image(&h, version)?;
    let crop = cimg::crop(&orig, rect);
    let pal = analyze::palette(&crop, 6);
    let dominant = cimg::dominant_color(&crop, None);
    let mut out = json!({"rect_px": rect, "dominant": dominant, "palette": pal.colors});
    if let Some(c) = args["color"].as_str() {
        let tol = args["tolerance"].as_u64().unwrap_or(40) as u8;
        let mask = trace::color_mask(&crop, c, tol);
        let bbox = mask_bbox(&mask).map(|b| [b[0] + rect[0], b[1] + rect[1], b[2], b[3]]);
        out["color_bbox_px"] = json!(bbox);
    }
    Ok(ToolResult::json(out))
}

fn mask_bbox(mask: &image::GrayImage) -> Option<[u32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for (x, y, p) in mask.enumerate_pixels() {
        if p[0] > 127 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 == u32::MAX {
        None
    } else {
        Some([x0, y0, x1 - x0 + 1, y1 - y0 + 1])
    }
}

const TRACE_MAX_UPSAMPLED_PX: f64 = 16_000_000.0;

fn trace_factor_limit(rect: [u32; 4]) -> u32 {
    let area = f64::from(rect[2].max(1)) * f64::from(rect[3].max(1));
    ((TRACE_MAX_UPSAMPLED_PX / area).sqrt().floor() as u32).clamp(1, 8)
}

fn trace_region(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let f = facts(ctx)?;
    let version = version_of(ctx, args)?;
    let rect = arg_rect_px(args, "rect_px")?;
    let color = arg_str(args, "color")?;
    let tol = args["tolerance"].as_u64().unwrap_or(40) as u8;
    let up = (args["upsample"]
        .as_u64()
        .unwrap_or(u64::from(trace::SMOOTH_FACTOR))
        .clamp(1, 8) as u32)
        .min(trace_factor_limit(rect));
    let name = tidy_name(&arg_str(args, "name")?);
    let strategy: Strategy = serde_json::from_value(args["strategy"].clone()).unwrap_or(Strategy::TextTraced);
    let reason = args["strategy_reason"].as_str().unwrap_or("").to_string();
    let group = args["group"].as_str().map(|s| s.to_string());
    let crop = {
        let h = img::lock(&ctx.handle);
        cimg::crop(&img::original_image(&h, version)?, rect)
    };
    let coverage = trace::color_coverage(&crop, &color, tol);
    let mask_up = trace::smooth_mask(&coverage, up, trace::SMOOTH_SIGMA_SOURCE_PX);
    let opts = trace::TraceOptions::upsampled(f.mm_per_px, up);
    let d = trace::trace_mask(&mask_up, &opts)?;
    let back = trace::rasterize_path(&d, mask_up.width(), mask_up.height(), 1.0 / opts.mm_per_px)?;
    let iou = hoist_core::compare::mask_overlap(&mask_up, &back);
    let layer_id = {
        let mut h = img::lock(&ctx.handle);
        let parent = match group {
            Some(g) => Some(ensure_group(&mut h, &g, &ctx.turn)?),
            None => None,
        };
        let id = h.document.next_id();
        let layer = Layer {
            id: id.clone(),
            name: name.clone(),
            parent,
            kind: LayerKind::Path(PathProps {
                d: d.clone(),
                fill_rule: FillRule::Nonzero,
            }),
            x_mm: f.offset_x_mm + rect[0] as f64 * f.mm_per_px,
            y_mm: f.offset_y_mm + rect[1] as f64 * f.mm_per_px,
            w_mm: rect[2] as f64 * f.mm_per_px,
            h_mm: rect[3] as f64 * f.mm_per_px,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![Paint::Solid {
                color: color.clone(),
                opacity: 1.0,
            }],
            strokes: vec![],
            effects: vec![],
            strategy,
            strategy_reason: if reason.is_empty() {
                format!("원본 모양 트레이싱 (iou {:.2})", iou)
            } else {
                reason
            },
            pinned: vec![],
            source: Some(LayerSource {
                original_version: Some(version),
                crop_px: Some(rect),
                asset: None,
                prompt: None,
            }),
        };
        ensure_running(ctx)?;
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(layer),
                index: None,
            }],
            Actor::Agent,
            Some(ctx.turn.clone()),
        )?;
        id
    };
    ctx.state.edits.fetch_add(1, Ordering::SeqCst);
    let mut res = edit_result(ctx, &layer_id, &format!("레이어 {layer_id} 트레이싱 추가"))?;
    let mut v: Value = serde_json::from_str(&res.text).unwrap_or(Value::Null);
    v["layer_id"] = json!(layer_id);
    v["iou"] = json!(r3(iou));
    v["path_len"] = json!(d.len());
    if iou < 0.9 {
        v["hint"] =
            json!("iou가 0.9 미만입니다. tolerance를 조정해 다시 따거나 generated_raster로 승격하세요.");
    }
    res.text = v.to_string();
    Ok(res)
}

fn crop_original(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let version = version_of(ctx, args)?;
    let rect = arg_rect_px(args, "rect_px")?;
    let name = sanitize(&arg_str(args, "name")?);
    let h = img::lock(&ctx.handle);
    let orig = img::original_image(&h, version)?;
    let mut crop = cimg::crop(&orig, rect);
    if let Some(c) = args["transparent_color"].as_str() {
        let tol = args["tolerance"].as_u64().unwrap_or(30) as u8;
        let mask = trace::color_mask(&crop, c, tol);
        for (x, y, p) in crop.enumerate_pixels_mut() {
            if mask.get_pixel(x, y)[0] > 127 {
                p[3] = 0;
            }
        }
    }
    let asset = h.new_asset_name(&name, "png");
    cimg::save_png(&crop, &h.asset_path(&asset))?;
    let f = facts(ctx)?;
    Ok(ToolResult { cached: false,
        text: json!({"asset": asset, "width_px": crop.width(), "height_px": crop.height(), "place_mm": [f.offset_x_mm + rect[0] as f64 * f.mm_per_px, f.offset_y_mm + rect[1] as f64 * f.mm_per_px, rect[2] as f64 * f.mm_per_px, rect[3] as f64 * f.mm_per_px]}).to_string(),
        images: vec![cimg::to_png_bytes(&cimg::resize_max(&crop, 512))?],
    })
}

fn cut_out(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    if !matting::available(&ctx.models_dir) {
        bail!(
            "누끼 모델이 설치돼 있지 않아 cut_out을 쓸 수 없습니다. 사용자에게는 'Hoist를 다시 설치하면 누끼 따기가 됩니다'라고 알리고, 이번에는 crop_original(transparent_color)이나 generate_image(transparent=true)로 대신하세요."
        );
    }
    let version = version_of(ctx, args)?;
    let rect = arg_rect_px(args, "rect_px")?;
    let name = sanitize(&arg_str(args, "name")?);
    let (crop, target_dpi) = {
        let h = img::lock(&ctx.handle);
        (
            cimg::crop(&img::original_image(&h, version)?, rect),
            h.project.target_dpi,
        )
    };
    if crop.width() == 0 || crop.height() == 0 {
        bail!("rect_px가 원본 밖입니다");
    }
    let cut = matting::remove_background(&ctx.models_dir, &crop)?;
    let asset = {
        let h = img::lock(&ctx.handle);
        let asset = h.new_asset_name(&name, "png");
        cimg::save_png(&cut, &h.asset_path(&asset))?;
        asset
    };
    let f = facts(ctx)?;
    let place = [
        f.offset_x_mm + f64::from(rect[0]) * f.mm_per_px,
        f.offset_y_mm + f64::from(rect[1]) * f.mm_per_px,
        f64::from(cut.width()) * f.mm_per_px,
        f64::from(cut.height()) * f.mm_per_px,
    ];
    let dpi = f64::from(cut.width()) * 25.4 / place[2];
    let note = if dpi < target_dpi {
        format!(
            "Print resolution {dpi:.0} dpi is below the target {target_dpi:.0} dpi. Do not enlarge this piece: redraw it with generate_image(transparent=true, references=[this asset], same aspect, long edge 3840 px), then compare the result with the piece and redraw if shape or colors differ."
        )
    } else {
        "Place it with add_layer(kind=image, strategy=original_crop, strategy_reason='원본에서 따서 배경 제거') at place_mm.".into()
    };
    Ok(ToolResult {
        cached: false,
        text: json!({"asset": asset, "width_px": cut.width(), "height_px": cut.height(), "place_mm": place, "dpi": dpi.round(), "note": note}).to_string(),
        images: vec![cimg::to_png_bytes(&cimg::resize_max(&cut, 512))?],
    })
}

const CLEAN_SUFFIX: &str = "clean, no noise, no dust, no artifacts";

fn clean_prompt(prompt: &str) -> String {
    format!("{}. {CLEAN_SUFFIX}", prompt.trim_end().trim_end_matches('.'))
}

fn sanitize(name: &str) -> String {
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

fn snap16(v: u32) -> u32 {
    ((v + 8) / 16 * 16).max(16)
}

const MAX_GEN_PX: u32 = 8_000_000;

fn clamp_gen_size(w: u32, h: u32) -> (u32, u32) {
    let mut w = w as f64;
    let mut h = h as f64;
    let ratio = w / h;
    if ratio > 3.0 {
        h = w / 3.0;
    } else if ratio < 1.0 / 3.0 {
        w = h / 3.0;
    }
    let long = w.max(h);
    if long > 3840.0 {
        let s = 3840.0 / long;
        w *= s;
        h *= s;
    }
    let px = w * h;
    if px > f64::from(MAX_GEN_PX) {
        let s = (f64::from(MAX_GEN_PX) / px).sqrt();
        w *= s;
        h *= s;
    }
    if px < 655_360.0 {
        let s = (655_360.0 / px).sqrt();
        w *= s;
        h *= s;
    }
    let (mut w, mut h) = (snap16(w.round() as u32), snap16(h.round() as u32));
    while w * h > MAX_GEN_PX {
        if w >= h {
            w -= 16;
        } else {
            h -= 16;
        }
    }
    (w, h)
}

fn plan_quality(ctx: &ToolCtx, pick: fn(&PlanQuality) -> Quality) -> Option<String> {
    let plan = ctx.state.plan.lock().unwrap_or_else(|e| e.into_inner());
    plan.as_ref().map(|p| pick(&p.image_quality).as_str().to_string())
}

/// The image API rejects some sizes we compute as over its pixel budget; one retry at half the area.
async fn request_image(ctx: &ToolCtx, req: ImageRequest) -> anyhow::Result<crate::openai::ImageOut> {
    match ctx.openai.image(req.clone()).await {
        Err(e) if e.to_string().contains("pixel budget") && req.size.is_some() => {
            let (w, h) = req.size.unwrap_or_default();
            let s = std::f64::consts::FRAC_1_SQRT_2;
            let size = clamp_gen_size((f64::from(w) * s) as u32, (f64::from(h) * s) as u32);
            log::warn!(
                "image size {w}x{h} over the pixel budget, retrying at {}x{}",
                size.0,
                size.1
            );
            ctx.openai
                .image(ImageRequest {
                    size: Some(size),
                    ..req
                })
                .await
        }
        other => other,
    }
}

fn resolve_reference(h: &ProjectHandle, facts: Option<&OriginalFacts>, r: &str) -> anyhow::Result<Vec<u8>> {
    if r == "original" {
        let v = facts.ok_or_else(|| anyhow!("원본 없음"))?.version;
        let im = img::original_image(h, v)?;
        return Ok(cimg::to_png_bytes(&im)?);
    }
    if let Some(rest) = r.strip_prefix("original:") {
        let v = facts.ok_or_else(|| anyhow!("원본 없음"))?.version;
        let nums: Vec<u32> = rest.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if nums.len() != 4 {
            bail!("original:x,y,w,h 형식이어야 합니다");
        }
        let im = img::original_image(h, v)?;
        return Ok(cimg::to_png_bytes(&cimg::crop(
            &im,
            [nums[0], nums[1], nums[2], nums[3]],
        ))?);
    }
    let path = h.asset_path(r);
    if !path.exists() {
        bail!("asset {r} 없음");
    }
    Ok(std::fs::read(path)?)
}

async fn generate_image(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let prompt = arg_str(args, "prompt")?;
    let name = sanitize(&arg_str(args, "name")?);
    let quality = args["quality"]
        .as_str()
        .map(str::to_string)
        .or_else(|| plan_quality(ctx, |q| q.figure))
        .unwrap_or_else(|| "high".into());
    let transparent = args["transparent"].as_bool().unwrap_or(false);
    let refs: Vec<String> = args["references"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let (references, mask_png, size) = {
        let h = img::lock(&ctx.handle);
        let mut references = Vec::new();
        for r in &refs {
            references.push(resolve_reference(&h, ctx.facts.as_ref(), r)?);
        }
        let first_dims = references
            .first()
            .and_then(|b| cimg::from_png_bytes(b).ok())
            .map(|i| (i.width(), i.height()));
        let mask_png = match (args["mask_rects_px"].as_array(), first_dims) {
            (Some(rects), Some((w, hh))) => {
                let mut mask = RgbaImage::from_pixel(w, hh, image::Rgba([0, 0, 0, 255]));
                for r in rects {
                    if let Some(a) = r.as_array()
                        && a.len() == 4
                    {
                        let x = a[0].as_f64().unwrap_or(0.0).max(0.0) as u32;
                        let y = a[1].as_f64().unwrap_or(0.0).max(0.0) as u32;
                        let rw = a[2].as_f64().unwrap_or(0.0).max(0.0) as u32;
                        let rh = a[3].as_f64().unwrap_or(0.0).max(0.0) as u32;
                        for yy in y..(y + rh).min(hh) {
                            for xx in x..(x + rw).min(w) {
                                mask.put_pixel(xx, yy, image::Rgba([0, 0, 0, 0]));
                            }
                        }
                    }
                }
                Some(cimg::to_png_bytes(&mask)?)
            }
            _ => None,
        };
        let size = match (args["width_px"].as_u64(), args["height_px"].as_u64()) {
            (Some(w), Some(hh)) => Some(clamp_gen_size(w as u32, hh as u32)),
            _ => first_dims.map(|(w, hh)| {
                clamp_gen_size(
                    w.max(1024),
                    (hh as f64 * (w.max(1024) as f64 / w as f64)).round() as u32,
                )
            }),
        };
        (references, mask_png, size)
    };
    let out = request_image(
        ctx,
        ImageRequest {
            model: ctx.image_model.clone(),
            prompt: clean_prompt(&prompt),
            size,
            quality,
            transparent,
            references,
            mask_png,
        },
    )
    .await?;
    *ctx.state.cost_krw.lock().unwrap_or_else(|e| e.into_inner()) += image_cost_krw(&out.usage);
    let im = cimg::from_png_bytes(&out.png)?;
    let (asset, preview) = {
        let h = img::lock(&ctx.handle);
        let asset = h.new_asset_name(&name, "png");
        std::fs::write(h.asset_path(&asset), &out.png)?;
        (asset, cimg::to_png_bytes(&cimg::resize_max(&im, 768))?)
    };
    ctx.state.edits.fetch_add(0, Ordering::SeqCst);
    Ok(ToolResult { cached: false, text: json!({"asset": asset, "width_px": im.width(), "height_px": im.height(), "transparent": transparent, "prompt": prompt}).to_string(), images: vec![preview] })
}

async fn extend_background(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let asset = arg_str(args, "asset")?;
    let hint = args["prompt"].as_str().unwrap_or("").trim().to_string();
    let prompt = format!(
        "Continue the existing background seamlessly into the transparent area, as if the same picture simply kept going. Keep the same palette, gradients, lighting, horizon and texture. Any leaf, cloud, plant or shape that is cut off at the edge of the existing picture must continue naturally across the boundary and finish; where the edge is plain, stay plain and soft. No text, no logos, no borders, no new objects or motifs that are not already present. The second image shows the whole background so far for style reference.{}",
        if hint.is_empty() {
            String::new()
        } else {
            format!(" Scenery hint: {hint}.")
        }
    );
    // Default height: enough for the background dpi requirement (half the target) so one pass is enough.
    let needed_h = {
        let h = img::lock(&ctx.handle);
        (h.document.artboard.height_mm / 25.4 * h.project.target_dpi / 2.0).ceil() as u64
    };
    let target_h = extend_height(Some(
        args["height_px"].as_u64().unwrap_or(0).max(needed_h).max(1280),
    ));
    let quality = plan_quality(ctx, |q| q.background).unwrap_or_else(|| "medium".into());
    let (mut cur, ratio) = {
        let h = img::lock(&ctx.handle);
        let im = cimg::load(&h.asset_path(&asset))?;
        let ratio = h.document.artboard.width_mm / h.document.artboard.height_mm.max(1.0);
        let scaled = cimg::resize_to(
            &im,
            ((im.width() as f64) * target_h as f64 / im.height() as f64)
                .round()
                .max(1.0) as u32,
            target_h,
        );
        (scaled, ratio)
    };
    let target_w = (target_h as f64 * ratio).round() as u32;
    if cur.width() >= target_w {
        let x = (cur.width() - target_w) / 2;
        cur = cimg::crop(&cur, [x, 0, target_w, target_h]);
    } else {
        let prompt = clean_prompt(&prompt);
        cur = outpaint(cur, target_w, &ctx.run.stop, |canvas, mask, plate| {
            let req = ImageRequest {
                model: ctx.image_model.clone(),
                prompt: prompt.clone(),
                size: Some(canvas.dimensions()),
                quality: quality.clone(),
                transparent: false,
                references: vec![],
                mask_png: None,
            };
            async move {
                let req = ImageRequest {
                    references: vec![cimg::to_png_bytes(&canvas)?, cimg::to_png_bytes(&plate)?],
                    mask_png: Some(cimg::to_png_bytes(&mask)?),
                    ..req
                };
                let out = request_image(ctx, req).await?;
                *ctx.state.cost_krw.lock().unwrap_or_else(|e| e.into_inner()) += image_cost_krw(&out.usage);
                Ok(cimg::from_png_bytes(&out.png)?)
            }
        })
        .await?;
    }
    let (out_asset, preview) = {
        let h = img::lock(&ctx.handle);
        let name = h.new_asset_name("background", "png");
        cimg::save_png(&cur, &h.asset_path(&name))?;
        (name, cimg::to_png_bytes(&cimg::resize_max(&cur, 1024))?)
    };
    Ok(ToolResult { cached: false, text: json!({"asset": out_asset, "width_px": cur.width(), "height_px": cur.height(), "note": "Place it with add_layer(kind=image, strategy=background, parent '배경', x/y/w/h null) so it covers the whole artboard."}).to_string(), images: vec![preview] })
}

/// The image API wants both sides divisible by 16, so the strip height rounds down within 512..=2160.
fn extend_height(requested: Option<u64>) -> u32 {
    // Rounded up: rounding the dpi-required height down left the plate just under the target (49.7 of 50 dpi).
    (requested.unwrap_or(1280).clamp(512, 2160) as u32).div_ceil(16) * 16
}

const EXTEND_MIN_STEP: u32 = 16;
const MAX_EXTEND_CALLS: u32 = 12;
const FEATHER_MAX_PX: u32 = 768;

/// Widens `cur` to `target_w` by asking `generate` to paint masked side chunks that overlap the current edge.
/// `generate` also receives a small copy of the whole plate so far, for style reference. Each new chunk is
/// feathered into the existing edge over a band so no hard seam remains.
/// Ends on its own: a last gap under 16 px, the call cap, or a step that adds nothing is closed by resizing.
async fn outpaint<F, Fut>(
    mut cur: RgbaImage,
    target_w: u32,
    stop: &AtomicBool,
    mut generate: F,
) -> anyhow::Result<RgbaImage>
where
    F: FnMut(RgbaImage, RgbaImage, RgbaImage) -> Fut,
    Fut: Future<Output = anyhow::Result<RgbaImage>>,
{
    let target_h = cur.height();
    let max_w = snap16((3 * target_h).min(3840));
    let mut calls = 0;
    while cur.width() + EXTEND_MIN_STEP <= target_w && calls < MAX_EXTEND_CALLS {
        if stop.load(Ordering::SeqCst) {
            bail!("사용자가 작업을 중단했어요");
        }
        let before = cur.width();
        let remaining = target_w - cur.width();
        let (left, right) = if remaining < 2 * EXTEND_MIN_STEP {
            (0, remaining)
        } else {
            (remaining / 2, remaining - remaining / 2)
        };
        for (right_side, need) in [(true, right), (false, left)] {
            if need == 0 || calls >= MAX_EXTEND_CALLS {
                continue;
            }
            let overlap = cur.width().min(target_h);
            let chunk_w = ((overlap + need).div_ceil(16) * 16).min(max_w);
            let new_px = chunk_w.saturating_sub(overlap);
            if new_px == 0 {
                continue;
            }
            let mut canvas = RgbaImage::from_pixel(chunk_w, target_h, image::Rgba([0, 0, 0, 0]));
            let mut mask = canvas.clone();
            for y in 0..target_h {
                for i in 0..overlap {
                    let (src_x, dst_x) = if right_side {
                        (cur.width() - overlap + i, i)
                    } else {
                        (i, chunk_w - overlap + i)
                    };
                    canvas.put_pixel(dst_x, y, *cur.get_pixel(src_x, y));
                    mask.put_pixel(dst_x, y, image::Rgba([0, 0, 0, 255]));
                }
            }
            let generated = generate(canvas, mask, cimg::resize_max(&cur, 1024)).await?;
            calls += 1;
            let generated = if generated.dimensions() == (chunk_w, target_h) {
                generated
            } else {
                cimg::resize_to(&generated, chunk_w, target_h)
            };
            let take = new_px.min(need);
            let band = (overlap / 2).clamp(1, FEATHER_MAX_PX);
            let mut next = RgbaImage::new(cur.width() + take, target_h);
            let (cur_off, gen_src, gen_off) = if right_side {
                (0, overlap, cur.width())
            } else {
                (take, chunk_w - overlap - take, 0)
            };
            for y in 0..target_h {
                for x in 0..cur.width() {
                    next.put_pixel(x + cur_off, y, *cur.get_pixel(x, y));
                }
                for x in 0..take {
                    next.put_pixel(x + gen_off, y, *generated.get_pixel(gen_src + x, y));
                }
                // Feather the generated overlap into the existing edge so the splice line disappears.
                for i in 0..band {
                    let (cur_x, gen_x) = if right_side {
                        (cur.width() - band + i, overlap - band + i)
                    } else {
                        (band - 1 - i, chunk_w - overlap + band - 1 - i)
                    };
                    let t = f64::from(i + 1) / f64::from(band + 1);
                    let a = cur.get_pixel(cur_x, y).0;
                    let b = generated.get_pixel(gen_x, y).0;
                    let mix = |k: usize| (f64::from(a[k]) * (1.0 - t) + f64::from(b[k]) * t).round() as u8;
                    next.put_pixel(cur_x + cur_off, y, image::Rgba([mix(0), mix(1), mix(2), 255]));
                }
            }
            cur = next;
        }
        if cur.width() == before {
            break;
        }
    }
    if cur.width() != target_w {
        cur = cimg::resize_to(&cur, target_w, target_h);
    }
    Ok(cur)
}

struct Ask {
    text: String,
    options: Vec<String>,
    allow_free_text: bool,
    rect_px: Option<[u32; 4]>,
}

/// `{questions: [...]}` (1–3), or the older single `{question, options}`.
fn parse_questions(args: &Value) -> anyhow::Result<Vec<Ask>> {
    let raw: Vec<Value> = match args["questions"].as_array() {
        Some(qs) => qs.clone(),
        None => vec![json!({"text": args["question"], "options": args["options"]})],
    };
    if raw.is_empty() || raw.len() > 3 {
        bail!("questions에는 질문을 1~3개 넣으세요");
    }
    raw.iter()
        .map(|q| {
            let text = q["text"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| anyhow!("질문마다 text가 필요합니다"))?
                .to_string();
            let options: Vec<String> = q["options"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            if options.len() == 1 || options.len() > 4 {
                bail!("options는 2~4개로 주거나, 자유 입력만 받으려면 비워 두세요");
            }
            let rect_px = if q["rect_px"].is_null() {
                None
            } else {
                Some(arg_rect_px(q, "rect_px")?)
            };
            Ok(Ask {
                text,
                allow_free_text: options.is_empty() || q["allow_free_text"].as_bool().unwrap_or(false),
                options,
                rect_px,
            })
        })
        .collect()
}

const QUESTION_THUMB_PX: u32 = 512;

/// The asked-about region with a 10% margin, long side scaled to 512 px so small print stays legible.
fn question_thumbnail(orig: &RgbaImage, [x, y, w, h]: [u32; 4]) -> anyhow::Result<String> {
    let (mx, my) = (w.div_ceil(10), h.div_ceil(10));
    let crop = cimg::crop(
        orig,
        [x.saturating_sub(mx), y.saturating_sub(my), w + 2 * mx, h + 2 * my],
    );
    if crop.width() == 0 || crop.height() == 0 {
        bail!("rect_px가 원본 밖입니다");
    }
    let s = f64::from(QUESTION_THUMB_PX) / f64::from(crop.width().max(crop.height()));
    let thumb = cimg::resize_to(
        &crop,
        (f64::from(crop.width()) * s).round().max(1.0) as u32,
        (f64::from(crop.height()) * s).round().max(1.0) as u32,
    );
    Ok(img::data_url(&cimg::to_png_bytes(&thumb)?))
}

async fn ask_user(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let asks = parse_questions(args)?;
    let original = match (ctx.facts.as_ref(), asks.iter().any(|a| a.rect_px.is_some())) {
        (Some(f), true) => Some(img::original_image(&img::lock(&ctx.handle), f.version)?),
        _ => None,
    };
    let mut waiting = Vec::new();
    for a in &asks {
        let image_png = match (&original, a.rect_px) {
            (Some(o), Some(r)) => Some(question_thumbnail(o, r)?),
            _ => None,
        };
        let question = Question {
            id: uuid::Uuid::new_v4().simple().to_string(),
            text: a.text.clone(),
            options: a.options.clone(),
            allow_free_text: a.allow_free_text,
            image_png,
            auto_label: AUTO_LABEL.into(),
        };
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        ctx.run
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(question.id.clone(), tx);
        (ctx.on_question)(question);
        waiting.push((a.text.clone(), rx));
    }
    ctx.state.asked.store(true, Ordering::SeqCst);
    let mut answers = Vec::new();
    for (question, rx) in waiting {
        if ctx.run.stop.load(Ordering::SeqCst) {
            bail!("사용자가 작업을 중단했어요");
        }
        let answer = tokio::select! {
            a = rx => a.map_err(|_| anyhow!("질문이 취소되었어요"))?,
            _ = ctx.run.stop_notify.notified() => bail!("사용자가 작업을 중단했어요"),
        };
        answers.push((question, answer));
    }
    let mut out = json!({});
    let mut items = Vec::new();
    for (question, answer) in answers {
        if answer == AUTO_ANSWER {
            items.push(json!({"question": question, "answer": AUTO_LABEL, "note": "The user left this decision to you. Choose what looks most natural and say which you chose."}));
            continue;
        }
        if ctx.state.fit_pending.load(Ordering::SeqCst)
            && let Some(mode) = fit_mode_from_answer(&answer)
        {
            img::lock(&ctx.handle).set_fit_mode(mode)?;
            ctx.state.fit_pending.store(false, Ordering::SeqCst);
            out["fit_mode"] = json!(mode);
            out["note"] = json!(format!(
                "배치 방식은 저장됐습니다. set_fit_mode는 부르지 말고 바로 set_plan과 편집을 시작하세요.\n{}",
                prompt::fit_mode_instruction(mode)
            ));
        }
        items.push(json!({"question": question, "answer": answer}));
    }
    out["answers"] = json!(items);
    Ok(ToolResult::json(out))
}

/// The fit question offers '배경만 늘려 채우기' and '재배치'; the chosen option text decides the mode.
fn fit_mode_from_answer(answer: &str) -> Option<FitMode> {
    if answer.contains("재배치") {
        Some(FitMode::Relayout)
    } else if answer.contains("배경") && (answer.contains("늘") || answer.contains("채우")) {
        Some(FitMode::ExtendBackground)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn shown(text: &str) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            text: text.into(),
            images: vec![vec![1, 2, 3]],
            cached: false,
        })
    }

    #[test]
    fn edits_wait_for_the_fit_mode_answer() {
        let state = TurnState {
            asked: AtomicBool::new(false),
            must_ask_first: false,
            fit_pending: AtomicBool::new(true),
            edits: AtomicUsize::new(0),
            promoted: AtomicUsize::new(0),
            cost_krw: Mutex::new(0.0),
            memory_dirty: AtomicBool::new(false),
            round: AtomicUsize::new(0),
            memo: ReadMemo::default(),
            plan: Mutex::new(None),
            plan_pending: AtomicBool::new(false),
        };
        let err = guard(&state, "add_layer")
            .expect_err("edit before the answer")
            .to_string();
        assert!(err.contains("ask_user") && err.contains("fit mode"), "{err}");
        assert!(guard(&state, "trace_region").is_err());
        assert!(guard(&state, "view_original").is_ok());
        assert!(guard(&state, "ask_user").is_ok());
        assert!(guard(&state, "set_fit_mode").is_ok());
        state.asked.store(true, Ordering::SeqCst);
        assert!(guard(&state, "add_layer").is_err(), "asking alone is not enough");
        state.fit_pending.store(false, Ordering::SeqCst);
        assert!(guard(&state, "add_layer").is_ok());

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

    fn rect_layer(id: &str, x: f64) -> Layer {
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

    #[test]
    fn a_new_parent_group_and_its_layer_get_different_ids() {
        let root = std::env::temp_dir().join(format!("hoist-ids-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root)
            .create("ids", 600.0, 300.0)
            .expect("project");
        let mut layer = rect_layer("", 0.0);
        layer.parent = Some("배경".into());
        let id = insert_layer(&mut h, layer, Some(0), "t").expect("empty id");
        let group = |doc: &Document, name: &str| {
            doc.layers
                .iter()
                .find(|l| l.name == name)
                .map(|l| l.id.clone())
                .expect("group")
        };
        let background = group(&h.document, "배경");
        assert_ne!(id, background);
        assert_eq!(
            h.document.layer(&id).and_then(|l| l.parent.clone()),
            Some(background)
        );

        let mut layer = rect_layer("", 0.0);
        layer.id = h.document.next_id();
        layer.parent = Some("글자".into());
        let id = insert_layer(&mut h, layer, None, "t").expect("id the new group takes");
        assert_ne!(id, group(&h.document, "글자"));
        assert_eq!(h.document.layers.len(), 4);
    }

    #[test]
    fn generated_sizes_stay_inside_the_pixel_budget() {
        for (w, h) in [
            (3219, 3840),
            (3840, 3840),
            (3840, 1280),
            (2600, 3100),
            (1000, 1000),
            (16, 4000),
        ] {
            let (gw, gh) = clamp_gen_size(w, h);
            assert!(gw * gh <= MAX_GEN_PX, "{w}x{h} -> {gw}x{gh}");
            assert!(
                gw % 16 == 0 && gh % 16 == 0 && gw.max(gh) <= 3840,
                "{w}x{h} -> {gw}x{gh}"
            );
        }
    }

    #[test]
    fn questions_parse_in_both_shapes() {
        let many = parse_questions(&json!({"questions": [
            {"text": "날짜가 맞나요?", "options": null, "allow_free_text": true, "rect_px": [10, 20, 30, 40]},
            {"text": "글꼴은?", "options": ["고딕", "명조"], "allow_free_text": null, "rect_px": null},
        ]}))
        .expect("questions");
        assert_eq!(many.len(), 2);
        assert!(many[0].allow_free_text && many[0].options.is_empty());
        assert_eq!(many[0].rect_px, Some([10, 20, 30, 40]));
        assert!(!many[1].allow_free_text && many[1].rect_px.is_none());

        let legacy = parse_questions(&json!({"question": "이대로 둘까요?", "options": ["네", "아니요"]}))
            .expect("legacy");
        assert_eq!(legacy.len(), 1);
        assert_eq!(
            (legacy[0].text.as_str(), legacy[0].options.len()),
            ("이대로 둘까요?", 2)
        );
        assert!(!legacy[0].allow_free_text);

        assert!(parse_questions(&json!({"questions": []})).is_err());
        assert!(
            parse_questions(
                &json!({"questions": [{"text": "a"}, {"text": "b"}, {"text": "c"}, {"text": "d"}]})
            )
            .is_err()
        );
        assert!(parse_questions(&json!({"questions": [{"text": "a", "options": ["하나"]}]})).is_err());
        assert!(parse_questions(&json!({"questions": [{"text": " "}]})).is_err());
    }

    #[test]
    fn the_question_thumbnail_shows_the_region_with_a_margin() {
        let orig = RgbaImage::from_pixel(400, 200, image::Rgba([250, 250, 250, 255]));
        let url = question_thumbnail(&orig, [100, 50, 100, 20]).expect("thumb");
        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            url.strip_prefix("data:image/png;base64,").expect("data url"),
        )
        .expect("base64");
        let thumb = cimg::from_png_bytes(&png).expect("png");
        assert_eq!(thumb.width(), 512, "small regions are scaled up to 512 px");
        assert_eq!(
            thumb.height(),
            102,
            "120x24 crop keeps its aspect: {}",
            thumb.height()
        );
        assert!(question_thumbnail(&orig, [900, 900, 10, 10]).is_err());
    }

    #[tokio::test]
    async fn ask_user_waits_for_every_answer_and_passes_auto_on() {
        let mut ctx = cut_ctx(std::env::temp_dir().join("hoist-no-models"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Question>();
        ctx.on_question = Arc::new(move |q| {
            let _ = tx.send(q);
        });
        let args = json!({"questions": [
            {"text": "전화번호가 043-646-6665 맞나요?", "options": null, "allow_free_text": true, "rect_px": [400, 250, 200, 100]},
            {"text": "잎 장식을 살릴까요?", "options": ["살리기", "빼기"], "allow_free_text": null, "rect_px": null},
        ]});
        let run = ctx.run.clone();
        let answering = async move {
            let first = rx.recv().await.expect("first question");
            let second = rx.recv().await.expect("second question");
            assert!(
                first
                    .image_png
                    .as_deref()
                    .is_some_and(|u| u.starts_with("data:image/png;base64,"))
            );
            assert!(first.allow_free_text && second.image_png.is_none());
            assert_eq!(second.auto_label, AUTO_LABEL);
            assert!(run.answer(&second.id, AUTO_ANSWER));
            assert!(run.answer(&first.id, "043-646-6666"));
        };
        let (result, ()) = tokio::join!(ask_user(&ctx, &args), answering);
        let v: Value = serde_json::from_str(&result.expect("answers").text).expect("json");
        assert_eq!(v["answers"][0]["answer"], "043-646-6666");
        assert_eq!(v["answers"][1]["answer"], AUTO_LABEL);
        assert!(
            v["answers"][1]["note"]
                .as_str()
                .is_some_and(|n| n.contains("left this decision"))
        );
    }

    #[test]
    fn the_fit_answer_picks_the_mode() {
        assert_eq!(
            fit_mode_from_answer("재배치(배치를 바꿔 가독성을 높임)"),
            Some(FitMode::Relayout)
        );
        assert_eq!(
            fit_mode_from_answer("현수막 비율에 맞게 요소 재배치"),
            Some(FitMode::Relayout)
        );
        assert_eq!(
            fit_mode_from_answer("배경만 늘려 채우기(원본 배치 그대로)"),
            Some(FitMode::ExtendBackground)
        );
        assert_eq!(fit_mode_from_answer("파란색으로 해 주세요"), None);
    }

    #[test]
    fn set_plan_unlocks_edits_and_sets_image_quality() {
        let ctx = cut_ctx(std::env::temp_dir().join("hoist-no-models"));
        ctx.state.plan_pending.store(true, Ordering::SeqCst);
        let err = guard(&ctx.state, "add_layer")
            .expect_err("no plan yet")
            .to_string();
        assert!(err.contains("set_plan"), "{err}");
        assert!(guard(&ctx.state, "view_original").is_ok());
        assert!(guard(&ctx.state, "set_plan").is_ok());
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
        assert!(guard(&ctx.state, "add_layer").is_ok());
        assert_eq!(plan_quality(&ctx, |q| q.figure).as_deref(), Some("high"));
        assert_eq!(plan_quality(&ctx, |q| q.background).as_deref(), Some("medium"));
        let saved = super::super::load_agent_state(&img::lock(&ctx.handle));
        assert_eq!(saved["plan"]["plan"]["image_quality"]["figure"], "high");
        assert_eq!(saved["plan"]["turn"], "t");
    }

    fn cut_ctx(models_dir: PathBuf) -> ToolCtx {
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
            state: Arc::new(TurnState {
                asked: AtomicBool::new(false),
                must_ask_first: false,
                fit_pending: AtomicBool::new(false),
                edits: AtomicUsize::new(0),
                promoted: AtomicUsize::new(0),
                cost_krw: Mutex::new(0.0),
                memory_dirty: AtomicBool::new(false),
                round: AtomicUsize::new(0),
                memo: ReadMemo::default(),
                plan: Mutex::new(None),
                plan_pending: AtomicBool::new(false),
            }),
            on_question: Arc::new(|_| {}),
            models_dir,
        }
    }

    #[test]
    fn cut_out_without_the_model_says_how_to_continue() {
        let ctx = cut_ctx(std::env::temp_dir().join("hoist-no-models"));
        let err = execute_sync(&ctx, "cut_out", json!({"rect_px": [0, 0, 100, 100], "name": "x"}))
            .err()
            .expect("no model")
            .to_string();
        assert!(
            err.contains("다시 설치") && err.contains("crop_original"),
            "{err}"
        );
    }

    #[test]
    fn every_defined_tool_is_wired_to_an_executor() {
        let ctx = cut_ctx(std::env::temp_dir().join("hoist-no-models"));
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

    #[test]
    #[ignore = "needs scripts/fetch-models.ps1"]
    fn cut_out_removes_the_background_and_places_the_piece() {
        let ctx = cut_ctx(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models"));
        let args = json!({"rect_px": [250, 50, 500, 500], "name": "disc", "version": null});
        let res = execute_sync(&ctx, "cut_out", args.clone()).expect("cut_out");
        let v: Value = serde_json::from_str(&res.text).expect("json");
        assert_eq!(res.images.len(), 1);
        assert_eq!(
            (v["width_px"].as_u64(), v["height_px"].as_u64()),
            (Some(500), Some(500))
        );
        let place: Vec<f64> = serde_json::from_value(v["place_mm"].clone()).expect("place");
        assert_eq!(place, [150.0, 30.0, 300.0, 300.0]);
        assert_eq!(v["dpi"].as_f64(), Some(42.0));
        assert!(
            v["note"].as_str().is_some_and(|n| n.contains("generate_image")),
            "{v}"
        );
        let asset = v["asset"].as_str().expect("asset");
        let cut = cimg::load(&img::lock(&ctx.handle).asset_path(asset)).expect("saved");
        assert!(cut.get_pixel(250, 250)[3] > 230, "disc stays");
        assert!(cut.get_pixel(5, 5)[3] < 25, "background goes");
        let again = execute_sync(&ctx, "cut_out", args).expect("again");
        assert!(again.cached && again.images.is_empty());
        img::lock(&ctx.handle).project.target_dpi = 60.0;
        let short = execute_sync(
            &ctx,
            "cut_out",
            json!({"rect_px": [250, 50, 499, 500], "name": "disc"}),
        )
        .expect("below target");
        assert!(
            short.text.contains("generate_image"),
            "42dpi < 60dpi regenerates: {}",
            short.text
        );
        img::lock(&ctx.handle).project.target_dpi = 40.0;
        let sharp = execute_sync(
            &ctx,
            "cut_out",
            json!({"rect_px": [250, 50, 500, 499], "name": "disc"}),
        )
        .expect("enough dpi");
        assert!(sharp.text.contains("original_crop"), "{}", sharp.text);
    }

    #[test]
    fn path_tools_merge_shapes_and_outline_text() {
        let ctx = cut_ctx(std::env::temp_dir().join("hoist-no-models"));
        let shape = rect_layer;
        let mut words = shape("t", 50.0);
        words.kind = LayerKind::Text(TextProps {
            text: "가나".into(),
            font_family: "Pretendard".into(),
            font_size_mm: 30.0,
            ..Default::default()
        });
        let ops = ["a", "b"]
            .iter()
            .zip([50.0, 120.0])
            .map(|(id, x)| shape(id, x))
            .chain([words])
            .map(|l| Op::AddLayer {
                layer: Box::new(l),
                index: None,
            })
            .collect();
        img::lock(&ctx.handle)
            .apply_ops(ops, Actor::User, None)
            .expect("seed");

        execute_sync(
            &ctx,
            "path_boolean",
            json!({"layer_ids": ["a", "b"], "op": "union"}),
        )
        .expect("union");
        execute_sync(&ctx, "outline_text", json!({"layer_id": "t"})).expect("outline");
        let h = img::lock(&ctx.handle);
        assert!(matches!(
            h.document.layer("a").map(|l| &l.kind),
            Some(LayerKind::Path(_))
        ));
        assert!(h.document.layer("b").is_none(), "merged away");
        assert!(matches!(
            h.document.layer("t").map(|l| &l.kind),
            Some(LayerKind::Path(_))
        ));
        assert_eq!(ctx.state.edits.load(Ordering::SeqCst), 2);
        assert_eq!(
            label_for("path_boolean", &json!({"op": "subtract"})),
            "패스 합치기 · 빼기"
        );
    }

    #[test]
    fn arrange_row_and_fit_box_place_layers_by_their_ink() {
        let ctx = cut_ctx(std::env::temp_dir().join("hoist-no-models"));
        let sized = |id: &str, x: f64, w: f64| Layer {
            w_mm: w,
            ..rect_layer(id, x)
        };
        let mut words = rect_layer("t", 0.0);
        words.kind = LayerKind::Text(TextProps {
            text: "가나다 라마바".into(),
            font_family: "Pretendard".into(),
            font_size_mm: 30.0,
            ..Default::default()
        });
        words.w_mm = 80.0;
        let mut pill = sized("L9", 0.0, 10.0);
        pill.name = "알약".into();
        let mut pinned = sized("pin", 0.0, 10.0);
        pinned.pinned = vec!["x_mm".into()];
        let ops = [
            sized("a", 0.0, 100.0),
            sized("b", 200.0, 50.0),
            sized("c", 210.0, 30.0),
            words,
            pill,
            pinned,
        ]
        .into_iter()
        .map(|l| Op::AddLayer {
            layer: Box::new(l),
            index: None,
        })
        .collect();
        img::lock(&ctx.handle)
            .apply_ops(ops, Actor::User, None)
            .expect("seed");
        let x = |id: &str| img::lock(&ctx.handle).document.layer(id).expect("layer").x_mm;
        let run = |name: &str, args: Value| -> Value {
            serde_json::from_str(&execute_sync(&ctx, name, args).expect(name).text).expect("json")
        };

        let v = run(
            "arrange_row",
            json!({"items": [["a"], ["b", "c"]], "x0_mm": 100, "x1_mm": 500, "justify": "center", "gap_mm": 20}),
        );
        assert_eq!((x("a"), x("b"), x("c")), (215.0, 335.0, 345.0));
        assert_eq!(v["gap_centers_mm"], json!([325.0]), "{v}");
        assert_eq!(
            (v["content_mm"].as_f64(), v["free_mm"].as_f64()),
            (Some(150.0), Some(230.0))
        );
        assert_eq!(v["row_mm"], json!([215.0, 385.0]));
        assert_eq!(v["items"][1]["layers"], json!(["b", "c"]));
        assert!(v.get("overflow").is_none() && v["region_mm"].is_array(), "{v}");

        run(
            "arrange_row",
            json!({"items": [["a"], ["b", "c"]], "x0_mm": 0, "x1_mm": 600, "justify": "space_between", "gap_mm": null}),
        );
        assert_eq!((x("a"), x("b"), x("c")), (0.0, 550.0, 560.0));
        run(
            "arrange_row",
            json!({"items": [["b"]], "x0_mm": 0, "x1_mm": 600, "justify": "space_between", "gap_mm": null}),
        );
        assert_eq!(x("b"), 275.0, "a single item is centered");
        run(
            "arrange_row",
            json!({"items": [["a"]], "x0_mm": 0, "x1_mm": 600, "justify": "end", "gap_mm": 5}),
        );
        assert_eq!(x("a"), 500.0);
        let v = run(
            "arrange_row",
            json!({"items": [["a"], ["b"]], "x0_mm": 0, "x1_mm": 100, "justify": "start", "gap_mm": 20}),
        );
        assert!(
            v["overflow"]
                .as_str()
                .is_some_and(|o| o.starts_with("content is 70 mm wider")),
            "{v}"
        );

        let ink = ink_mm(&ctx.fonts, img::lock(&ctx.handle).document.layer("t").expect("t")).0;
        let v = run(
            "fit_box",
            json!({"id": "알약", "around": ["t"], "pad_x_mm": 10, "pad_y_mm": 5, "axis": "both"}),
        );
        assert_eq!(v["layer_id"], "L9");
        assert_eq!(
            v["box_mm"],
            json!([ink[0] - 10.0, ink[1] - 5.0, ink[2] + 20.0, ink[3] + 10.0].map(r1))
        );
        run(
            "fit_box",
            json!({"id": "L9", "around": ["a"], "pad_x_mm": 0, "pad_y_mm": 0, "axis": "x"}),
        );
        let l = img::lock(&ctx.handle).document.layer("L9").expect("pill").clone();
        assert_eq!((l.x_mm, l.w_mm), (x("a"), 100.0));
        assert!((l.y_mm - (ink[1] - 5.0)).abs() < 1e-9, "axis x keeps y");

        let v = run("update_layer", json!({"id": "t", "patch": {"x_mm": 12}}));
        assert_eq!(v["lines"], json!(2), "{v}");
        assert!(v["ink_mm"].as_array().is_some_and(|a| a.len() == 4), "{v}");

        let err = |name: &str, args: Value| execute_sync(&ctx, name, args).err().expect(name).to_string();
        assert!(
            err(
                "fit_box",
                json!({"id": "t", "around": ["a"], "pad_x_mm": 0, "pad_y_mm": 0, "axis": "both"})
            )
            .contains("not a shape")
        );
        assert!(
            err(
                "arrange_row",
                json!({"items": [["없음"]], "x0_mm": 0, "x1_mm": 9, "justify": "center", "gap_mm": 0})
            )
            .contains("'없음' not found")
        );
        assert!(
            err(
                "arrange_row",
                json!({"items": [["pin"]], "x0_mm": 0, "x1_mm": 600, "justify": "center", "gap_mm": 0})
            )
            .contains("pinned")
        );
        assert_eq!(ctx.state.edits.load(Ordering::SeqCst), 8);
        assert_eq!(
            label_for("arrange_row", &json!({"items": [["제목 1", "x"]]})),
            "줄 맞추기 · 제목 1"
        );
    }

    async fn painted(canvas: RgbaImage, _mask: RgbaImage, _plate: RgbaImage) -> anyhow::Result<RgbaImage> {
        Ok(RgbaImage::from_pixel(
            canvas.width(),
            canvas.height(),
            image::Rgba([200, 30, 30, 255]),
        ))
    }

    #[tokio::test]
    async fn outpaint_ends_for_every_remaining_width() {
        let stop = AtomicBool::new(false);
        for (start_w, h, extra) in [
            (100, 64, 1),
            (100, 64, 7),
            (100, 64, 15),
            (100, 64, 16),
            (100, 64, 17),
            (100, 64, 31),
            (100, 64, 1000),
            (100, 60, 1),
            (100, 60, 9),
            (100, 60, 500),
            (40, 64, 300),
        ] {
            let calls = Cell::new(0);
            let start = RgbaImage::from_pixel(start_w, h, image::Rgba([10, 120, 200, 255]));
            let out = outpaint(start, start_w + extra, &stop, |c, m, p| {
                calls.set(calls.get() + 1);
                painted(c, m, p)
            })
            .await
            .expect("outpaint");
            assert_eq!(
                out.dimensions(),
                (start_w + extra, h),
                "start {start_w} h {h} +{extra}"
            );
            assert!(
                calls.get() <= MAX_EXTEND_CALLS,
                "start {start_w} h {h} +{extra}: {} calls",
                calls.get()
            );
            if extra < EXTEND_MIN_STEP {
                assert_eq!(calls.get(), 0, "a gap under 16 px is closed without the API");
            }
        }
    }

    #[test]
    fn extend_height_is_a_multiple_of_16_within_limits() {
        assert_eq!(extend_height(Some(1400)), 1408);
        assert_eq!(extend_height(Some(1772)), 1776);
        assert_eq!(extend_height(Some(2160)), 2160);
        assert_eq!(extend_height(Some(5000)), 2160);
        assert_eq!(extend_height(Some(100)), 512);
        assert_eq!(extend_height(None), 1280);
        assert!((512..=2160u64).all(|h| extend_height(Some(h)).is_multiple_of(16)));
    }

    #[tokio::test]
    async fn outpaint_stops_when_asked() {
        let stop = AtomicBool::new(true);
        let start = RgbaImage::from_pixel(100, 64, image::Rgba([0, 0, 0, 255]));
        let err = outpaint(start, 1000, &stop, painted).await.expect_err("stopped");
        assert!(err.to_string().contains("중단"), "{err}");
    }
}
