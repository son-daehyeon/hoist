use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail};
use hoist_core::check::export_check;
use hoist_core::model::*;
use hoist_core::render::layer_bounds_mm;
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;
use hoist_core::{analyze, image as cimg, trace};
use image::RgbaImage;
use serde_json::{Value, json};

use super::AgentRun;
use super::prompt::{self, OriginalFacts};
use super::{img, layer_args};
use crate::openai::{ImageRequest, OpenAI, image_cost_krw};

pub struct TurnState {
    pub asked: AtomicBool,
    pub must_ask_first: bool,
    pub edits: AtomicUsize,
    pub promoted: AtomicUsize,
    pub cost_krw: Mutex<f64>,
    pub memory_dirty: AtomicBool,
    pub round: AtomicUsize,
    pub memo: ReadMemo,
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
    "compare",
    "analyze_original",
    "measure",
    "crop_original",
];

pub struct ToolCtx {
    pub handle: Arc<Mutex<ProjectHandle>>,
    pub fonts: Arc<Fonts>,
    pub openai: OpenAI,
    pub image_model: String,
    pub turn: String,
    pub facts: Option<OriginalFacts>,
    pub run: Arc<AgentRun>,
    pub state: Arc<TurnState>,
    pub on_question: Arc<dyn Fn(String, String, Vec<String>) + Send + Sync>,
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
    "trace_region",
    "generate_image",
    "extend_background",
    "crop_original",
];

pub fn definitions() -> Vec<Value> {
    let f = |name: &str, desc: &str, props: Value, required: &[&str]| json!({"type":"function","name":name,"description":desc,"parameters":{"type":"object","properties":props,"required":required,"additionalProperties":false}});
    let rect_px = json!({"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4,"description":"[x, y, w, h] 원본 픽셀"});
    let rect_mm = json!({"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"[x, y, w, h] mm"});
    vec![
        f(
            "get_document",
            "현재 문서(아트보드와 모든 레이어 요약)를 돌려줍니다.",
            json!({}),
            &[],
        ),
        f(
            "get_layer",
            "레이어 하나의 전체 JSON을 돌려줍니다.",
            json!({"id":{"type":"string"}}),
            &["id"],
        ),
        f(
            "view_original",
            "원본 이미지(또는 그 일부)를 봅니다. rect_px를 주면 그 부분만 확대해서 봅니다.",
            json!({"version":{"type":["integer","null"]},"rect_px":{"anyOf":[rect_px.clone(),{"type":"null"}]},"max_px":{"type":["integer","null"],"description":"긴 변 최대 px (기본 1536)"}}),
            &[],
        ),
        f(
            "view_work",
            "작업본(현재 문서)을 렌더링해서 봅니다. rect_mm을 주면 그 영역만, layer_ids를 주면 그 레이어만 봅니다.",
            json!({"rect_mm":{"anyOf":[rect_mm.clone(),{"type":"null"}]},"layer_ids":{"type":["array","null"],"items":{"type":"string"}},"max_px":{"type":["integer","null"]}}),
            &[],
        ),
        f(
            "compare",
            "원본과 작업본을 나란히 놓고 비교합니다. 점수(similarity 0~1, overlap, edge_similarity)와 차이가 큰 영역(worst_regions, 0~1 비율 좌표)을 돌려줍니다.",
            json!({"rect_mm":{"anyOf":[rect_mm.clone(),{"type":"null"}]}}),
            &[],
        ),
        f(
            "analyze_original",
            "원본의 팔레트와 영역(글자/도형/사진/배경 후보, bbox_px)을 돌려줍니다.",
            json!({"version":{"type":["integer","null"]}}),
            &[],
        ),
        f(
            "measure",
            "원본의 한 영역에서 주된 색, 팔레트, 색 영역의 bbox를 잽니다.",
            json!({"rect_px":rect_px.clone(),"version":{"type":["integer","null"]},"color":{"type":["string","null"],"description":"이 색의 bbox를 잴 때 #RRGGBB"},"tolerance":{"type":["integer","null"]}}),
            &["rect_px"],
        ),
        layer_args::add_layer_tool(true),
        layer_args::update_layer_tool(true),
        f(
            "delete_layer",
            "레이어를 지웁니다 (그룹이면 자식도).",
            json!({"id":{"type":"string"}}),
            &["id"],
        ),
        f(
            "move_layer",
            "레이어의 부모나 z-순서를 바꿉니다. index 0이 맨 아래.",
            json!({"id":{"type":"string"},"parent":{"type":["string","null"]},"index":{"type":"integer"}}),
            &["id", "index"],
        ),
        f(
            "trace_region",
            "원본의 영역에서 color와 비슷한 픽셀을 벡터 path로 따서 레이어로 추가합니다. 글자 채움색이나 단색 도형에 씁니다. iou(마스크 겹침)와 렌더를 돌려줍니다.",
            json!({"rect_px":rect_px.clone(),"color":{"type":"string","description":"#RRGGBB"},"tolerance":{"type":["integer","null"],"description":"채널당 허용 오차, 기본 40"},"upsample":{"type":["integer","null"],"description":"트레이싱 전 확대 배수, 기본 8 (큰 영역은 자동으로 줄어듦)"},"name":{"type":"string"},"group":{"type":["string","null"],"description":"부모 그룹 이름 (없으면 만듦)"},"strategy":{"type":"string","enum":["text_traced","traced_vector","native_vector"]},"strategy_reason":{"type":"string"},"version":{"type":["integer","null"]}}),
            &["rect_px", "color", "name", "strategy", "strategy_reason"],
        ),
        f(
            "crop_original",
            "원본의 영역을 잘라 assets에 저장합니다. transparent_color를 주면 그 색을 투명하게 만듭니다. 결과 asset 이름과 크기를 돌려줍니다.",
            json!({"rect_px":rect_px.clone(),"name":{"type":"string"},"version":{"type":["integer","null"]},"transparent_color":{"type":["string","null"]},"tolerance":{"type":["integer","null"]}}),
            &["rect_px", "name"],
        ),
        f(
            "generate_image",
            "OpenAI 이미지 모델로 그림을 만들거나 고칩니다. references에 asset 이름, 'original', 'original:x,y,w,h'(원본 조각)를 넣으면 그 이미지를 참고해 다시 그립니다. 결과를 assets에 저장하고 이름·크기·미리보기를 돌려줍니다. 객체 하나씩, transparent=true로 만들면 바로 레이어로 쓸 수 있습니다.",
            json!({"prompt":{"type":"string"},"name":{"type":"string"},"width_px":{"type":["integer","null"]},"height_px":{"type":["integer","null"]},"quality":{"type":["string","null"],"enum":["low","medium","high","xhigh","max",null]},"transparent":{"type":["boolean","null"]},"references":{"type":["array","null"],"items":{"type":"string"}},"mask_rects_px":{"type":["array","null"],"items":rect_px.clone(),"description":"첫 참조 이미지에서 다시 그릴 영역들 (마스크)"}}),
            &["prompt", "name"],
        ),
        f(
            "extend_background",
            "배경 asset을 현수막 비율까지 양옆으로 이어 그립니다. 결과 asset 이름과 크기를 돌려줍니다.",
            json!({"asset":{"type":"string"},"prompt":{"type":["string","null"]},"height_px":{"type":["integer","null"],"description":"결과 높이 px, 기본 1280"}}),
            &["asset"],
        ),
        f("list_fonts", "쓸 수 있는 글꼴 이름 목록.", json!({}), &[]),
        f(
            "export_check",
            "실물 기준 해상도가 모자란 이미지 레이어 목록.",
            json!({"dpi":{"type":["number","null"]}}),
            &[],
        ),
        f(
            "set_memory",
            "프로젝트 메모리(확정된 사실)를 통째로 바꿉니다.",
            json!({"text":{"type":"string"}}),
            &["text"],
        ),
        f(
            "append_memory",
            "프로젝트 메모리에 한 줄을 추가합니다.",
            json!({"line":{"type":"string"}}),
            &["line"],
        ),
        f(
            "set_fit_mode",
            "원본과 현수막 비율이 다를 때 배치 방식을 프로젝트에 저장합니다. ask_user로 사용자 답을 받은 뒤 호출합니다.",
            json!({"mode":{"type":"string","enum":["extend_background","relayout"],"description":"extend_background: 배경만 늘려 채우기, relayout: 현수막 비율에 맞게 요소 재배치"}}),
            &["mode"],
        ),
        f(
            "ask_user",
            "사용자에게 선택지를 묻고 답을 기다립니다.",
            json!({"question":{"type":"string"},"options":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":4}}),
            &["question", "options"],
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
        "compare" => "원본과 비교".into(),
        "analyze_original" => "원본 분석".into(),
        "measure" => "색·크기 재기".into(),
        "add_layer" => format!("레이어 추가 · {}", args["layer"]["name"].as_str().unwrap_or("")),
        "update_layer" => format!("레이어 고치기 · {}", s("id")),
        "delete_layer" => format!("레이어 지우기 · {}", s("id")),
        "move_layer" => format!("레이어 순서 · {}", s("id")),
        "trace_region" => format!("모양 따기 · {}", s("name")),
        "crop_original" => format!("원본 조각 자르기 · {}", s("name")),
        "generate_image" => format!("그림 만들기 · {}", s("name")),
        "extend_background" => "배경 이어 그리기".into(),
        "list_fonts" => "글꼴 목록".into(),
        "export_check" => "출력 해상도 점검".into(),
        "set_memory" | "append_memory" => "메모리 기록".into(),
        "ask_user" => "사용자에게 묻기".into(),
        "set_fit_mode" => "배치 방식 저장".into(),
        _ => name.to_string(),
    }
}

pub async fn execute(ctx: &ToolCtx, name: &str, args: Value) -> anyhow::Result<ToolResult> {
    if ctx.state.must_ask_first && !ctx.state.asked.load(Ordering::SeqCst) && EDIT_TOOLS.contains(&name) {
        bail!("원본이 바뀌었으니 먼저 ask_user로 사용자에게 어떻게 할지 물어야 합니다.");
    }
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
            let index = args["index"].as_u64().map(|v| v as usize);
            let touched = {
                let mut h = img::lock(&ctx.handle);
                if layer.id.is_empty() || h.document.layer(&layer.id).is_some() {
                    layer.id = h.document.next_id();
                }
                if let Some(p) = &layer.parent
                    && h.document.layer(p).is_none()
                {
                    let name = p.clone();
                    let gid = ensure_group(&mut h, &name, &ctx.turn)?;
                    layer.parent = Some(gid);
                }
                let id = layer.id.clone();
                h.apply_ops(
                    vec![Op::AddLayer {
                        layer: Box::new(layer),
                        index,
                    }],
                    Actor::Agent,
                    Some(ctx.turn.clone()),
                )?;
                id
            };
            ctx.state.edits.fetch_add(1, Ordering::SeqCst);
            edit_result(ctx, &touched, &format!("레이어 {touched} 추가"))
        }
        "update_layer" => {
            let id = arg_str(&args, "id")?;
            let patch = layer_args::patch_from_args(&args["patch"]);
            if patch.as_object().is_none_or(|m| m.is_empty()) {
                bail!("patch에 바꿀 필드가 없습니다. 바꿀 필드만 값을 넣고 나머지는 null로 보내세요.");
            }
            if patch.get("strategy").is_some() {
                ctx.state.promoted.fetch_add(1, Ordering::SeqCst);
            }
            {
                let mut h = img::lock(&ctx.handle);
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
        "delete_layer" => {
            let id = arg_str(&args, "id")?;
            let bounds = {
                let mut h = img::lock(&ctx.handle);
                let b = h.document.layer(&id).map(|l| layer_bounds_mm(&h.document, l));
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
        "trace_region" => trace_region(ctx, &args),
        "generate_image" => generate_image(ctx, &args).await,
        "extend_background" => extend_background(ctx, &args).await,
        "list_fonts" => Ok(ToolResult::json(json!({"fonts": ctx.fonts.families()}))),
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
        "ask_user" => ask_user(ctx, &args).await,
        "set_fit_mode" => {
            let mode: FitMode = serde_json::from_value(args["mode"].clone())
                .map_err(|_| anyhow!("mode는 extend_background 또는 relayout이어야 합니다"))?;
            img::lock(&ctx.handle).set_fit_mode(mode)?;
            Ok(ToolResult::text(prompt::fit_mode_instruction(mode)))
        }
        _ => bail!("알 수 없는 도구 {name}"),
    }
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
        "compare" => compare_tool(ctx, args),
        "analyze_original" => analyze_original(ctx, args),
        "measure" => measure(ctx, args),
        "crop_original" => crop_original(ctx, args),
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
        "crop_original" => Some(original_region()),
        "get_layer" | "delete_layer" => layer("id").map(|n| format!("'{n}'")),
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
        "ask_user" => args["question"].as_str().map(|q| clip(q, 30)),
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

fn ensure_group(h: &mut ProjectHandle, name: &str, turn: &str) -> anyhow::Result<LayerId> {
    if let Some(g) = h
        .document
        .layers
        .iter()
        .find(|l| l.parent.is_none() && matches!(l.kind, LayerKind::Group) && l.name == name)
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
    let bounds = {
        let h = img::lock(&ctx.handle);
        let l = h
            .document
            .layer(layer_id)
            .ok_or_else(|| anyhow!("레이어 {layer_id} 없음"))?;
        layer_bounds_mm(&h.document, l)
    };
    region_result(ctx, bounds, note)
}

fn region_result(ctx: &ToolCtx, bounds: [f64; 4], note: &str) -> anyhow::Result<ToolResult> {
    let pad_w = bounds[2] * 0.15 + 10.0;
    let pad_h = bounds[3] * 0.15 + 10.0;
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
    match ctx.facts.as_ref() {
        Some(f) => {
            let (res, png) = img::compare_region(&h, &ctx.fonts, f, rect, 900)?;
            Ok(ToolResult { cached: false,
                text: json!({"note": note, "region_mm": rect, "similarity": r3(res.similarity), "overlap": r3(res.overlap), "edge_similarity": r3(res.edge_similarity), "image": "왼쪽 원본 · 오른쪽 작업본"}).to_string(),
                images: vec![png],
            })
        }
        None => {
            let im = img::work_region(&h, &ctx.fonts, rect, 900)?;
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
        img::work_region(&h, &ctx.fonts, img::clamp_rect_mm(&h, r), max_px)?
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
        let (res, png) = img::compare_region(&h, &ctx.fonts, f, rect, 1200)?;
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
    let name = arg_str(args, "name")?;
    let strategy: Strategy = serde_json::from_value(args["strategy"].clone()).unwrap_or(Strategy::TextTraced);
    let reason = args["strategy_reason"].as_str().unwrap_or("").to_string();
    let group = args["group"].as_str().map(|s| s.to_string());
    let (d, iou, layer_id) = {
        let mut h = img::lock(&ctx.handle);
        let orig = img::original_image(&h, version)?;
        let crop = cimg::crop(&orig, rect);
        let coverage = trace::color_coverage(&crop, &color, tol);
        let mask_up = trace::smooth_mask(&coverage, up, trace::SMOOTH_SIGMA_SOURCE_PX);
        let opts = trace::TraceOptions::upsampled(f.mm_per_px, up);
        let mm_per_px = opts.mm_per_px;
        let d = trace::trace_mask(&mask_up, &opts)?;
        let back = trace::rasterize_path(&d, mask_up.width(), mask_up.height(), 1.0 / mm_per_px)?;
        let iou = hoist_core::compare::mask_overlap(&mask_up, &back);
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
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(layer),
                index: None,
            }],
            Actor::Agent,
            Some(ctx.turn.clone()),
        )?;
        (d, iou, id)
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
    if px > 8_294_400.0 {
        let s = (8_294_400.0 / px).sqrt();
        w *= s;
        h *= s;
    }
    if px < 655_360.0 {
        let s = (655_360.0 / px).sqrt();
        w *= s;
        h *= s;
    }
    (snap16(w.round() as u32), snap16(h.round() as u32))
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
    let quality = args["quality"].as_str().unwrap_or("high").to_string();
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
    let out = ctx
        .openai
        .image(ImageRequest {
            model: ctx.image_model.clone(),
            prompt: clean_prompt(&prompt),
            size,
            quality,
            transparent,
            references,
            mask_png,
        })
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
    let prompt = args["prompt"].as_str().unwrap_or("Continue the same scenery seamlessly into the transparent area. Same style, colors, lighting and horizon. No text, no logos, no new objects.").to_string();
    let target_h = args["height_px"].as_u64().unwrap_or(1280).clamp(512, 2160) as u32;
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
        let mut calls = 0;
        while cur.width() < target_w && calls < 8 {
            let remaining = target_w - cur.width();
            let (left_part, right_part) = (remaining / 2, remaining - remaining / 2);
            for side in [1u8, 0u8] {
                let need = if side == 1 { right_part } else { left_part };
                if need == 0 || cur.width() >= target_w {
                    continue;
                }
                let overlap = cur.width().min(target_h);
                let max_w = snap16((3 * target_h).min(3840));
                let chunk_w = snap16((overlap + need).min(max_w));
                let new_px = chunk_w.saturating_sub(overlap);
                if new_px == 0 {
                    break;
                }
                let mut canvas = RgbaImage::from_pixel(chunk_w, target_h, image::Rgba([0, 0, 0, 0]));
                let mut mask = RgbaImage::from_pixel(chunk_w, target_h, image::Rgba([0, 0, 0, 0]));
                for y in 0..target_h {
                    for i in 0..overlap {
                        let (src_x, dst_x) = if side == 1 {
                            (cur.width() - overlap + i, i)
                        } else {
                            (i, chunk_w - overlap + i)
                        };
                        canvas.put_pixel(dst_x, y, *cur.get_pixel(src_x, y));
                        mask.put_pixel(dst_x, y, image::Rgba([0, 0, 0, 255]));
                    }
                }
                let out = ctx
                    .openai
                    .image(ImageRequest {
                        model: ctx.image_model.clone(),
                        prompt: clean_prompt(&prompt),
                        size: Some((chunk_w, target_h)),
                        quality: "high".into(),
                        transparent: false,
                        references: vec![cimg::to_png_bytes(&canvas)?],
                        mask_png: Some(cimg::to_png_bytes(&mask)?),
                    })
                    .await?;
                *ctx.state.cost_krw.lock().unwrap_or_else(|e| e.into_inner()) += image_cost_krw(&out.usage);
                calls += 1;
                let generated = cimg::from_png_bytes(&out.png)?;
                let generated = if generated.width() != chunk_w || generated.height() != target_h {
                    cimg::resize_to(&generated, chunk_w, target_h)
                } else {
                    generated
                };
                let take = new_px.min(need);
                let mut next = RgbaImage::new(cur.width() + take, target_h);
                let (cur_off, gen_src, gen_off) = if side == 1 {
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
                }
                cur = next;
            }
        }
    }
    let (out_asset, preview) = {
        let h = img::lock(&ctx.handle);
        let name = h.new_asset_name("background", "png");
        cimg::save_png(&cur, &h.asset_path(&name))?;
        (name, cimg::to_png_bytes(&cimg::resize_max(&cur, 1024))?)
    };
    Ok(ToolResult { cached: false, text: json!({"asset": out_asset, "width_px": cur.width(), "height_px": cur.height(), "note": "add_layer(kind=image, strategy=background)로 아트보드 전체(0,0,W,H mm)에 배치하세요"}).to_string(), images: vec![preview] })
}

async fn ask_user(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let question = arg_str(args, "question")?;
    let options: Vec<String> = args["options"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if options.len() < 2 {
        bail!("options는 2개 이상이어야 합니다");
    }
    let qid = uuid::Uuid::new_v4().simple().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    *ctx.run.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some((qid.clone(), tx));
    (ctx.on_question)(qid.clone(), question.clone(), options.clone());
    ctx.state.asked.store(true, Ordering::SeqCst);
    let answer = tokio::select! {
        a = rx => a.map_err(|_| anyhow!("질문이 취소되었어요"))?,
        _ = ctx.run.stop_notify.notified() => bail!("사용자가 작업을 중단했어요"),
    };
    Ok(ToolResult::json(json!({"question": question, "answer": answer})))
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
}
