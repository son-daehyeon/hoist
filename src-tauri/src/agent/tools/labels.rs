use hoist_core::model::Document;
use hoist_core::ops::{field_label, patch_paths};
use serde_json::Value;

use super::{arg_rect_mm, find_layer};
use crate::agent::layer_args;
use crate::agent::prompt::OriginalFacts;

pub fn label_for(name: &str, args: &Value) -> String {
    let s = |k: &str| args[k].as_str().unwrap_or("").to_string();
    match name {
        "get_document" => "문서 읽기".into(),
        "get_layer" => "레이어 보기".into(),
        "view_original" => "원본 보기".into(),
        "view_work" => "작업본 보기".into(),
        "font_lineup" => "글꼴 비교".into(),
        "compare" => "원본과 비교".into(),
        "analyze_original" => "원본 분석".into(),
        "measure" => "색·크기 재기".into(),
        "add_layer" => format!("레이어 추가 · {}", args["layer"]["name"].as_str().unwrap_or("")),
        "update_layer" => "레이어 고치기".into(),
        "delete_layer" => "레이어 지우기".into(),
        "move_layer" => "레이어 순서".into(),
        "arrange_row" => "줄 맞추기".into(),
        "fit_box" => "상자 맞추기".into(),
        "path_boolean" => format!(
            "패스 합치기 · {}",
            match s("op").as_str() {
                "subtract" => "빼기",
                "intersect" => "겹친 곳만",
                "exclude" => "겹친 곳 빼기",
                _ => "합치기",
            }
        ),
        "outline_text" => "글자 윤곽선".into(),
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
    format!("{place} 일부")
}

fn changed_words(patch: &Value) -> String {
    let mut words: Vec<&str> = Vec::new();
    for path in patch_paths(patch) {
        let word = match path.as_str() {
            "x_mm" | "y_mm" => "위치",
            "w_mm" | "h_mm" => "크기",
            p if p == "pinned" || p.starts_with("strategy") || p.starts_with("source") => continue,
            p => field_label(p),
        };
        if !words.contains(&word) {
            words.push(word);
        }
    }
    words.sort_by_key(|w| match *w {
        "위치" => 0,
        "크기" => 1,
        _ => 2,
    });
    let shown = words.len().min(4);
    let mut out = words[..shown].join("·");
    if words.len() > shown {
        out.push_str(&format!(" 외 {}", words.len() - shown));
    }
    out
}

pub fn detail_for(name: &str, args: &Value, doc: &Document, facts: Option<&OriginalFacts>) -> Option<String> {
    let named = |r: &str| find_layer(doc, r).map_or_else(|_| r.to_string(), |l| l.name.clone());
    let layer = |key: &str| args[key].as_str().map(named);
    let original_region = || -> String {
        match (arg_rect_mm(args, "rect_px"), facts) {
            (Some(r), Some(f)) => region_words(r, f.width_px as f64, f.height_px as f64),
            (Some(_), None) => "일부".into(),
            (None, _) => "전체".into(),
        }
    };
    let work_region = || -> String {
        match arg_rect_mm(args, "rect_mm") {
            Some(r) => region_words(r, doc.artboard.width_mm, doc.artboard.height_mm),
            None => "전체".into(),
        }
    };
    match name {
        "view_original" => Some(original_region()),
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
            None => Some(work_region()),
        },
        "compare" => Some(work_region()),
        "measure" | "trace_region" | "crop_original" | "cut_out" => Some(original_region()),
        "get_layer" | "delete_layer" | "fit_box" => layer("id"),
        "outline_text" => layer("layer_id"),
        "arrange_row" => args["items"].as_array().map(|items| {
            let names: Vec<String> = items
                .iter()
                .filter_map(|item| item[0].as_str())
                .map(named)
                .collect();
            clip(&names.join(", "), 40)
        }),
        "path_boolean" => args["layer_ids"].as_array().map(|ids| {
            let names: Vec<String> = ids
                .iter()
                .filter_map(Value::as_str)
                .map(|id| doc.layer(id).map_or_else(|| id.to_string(), |l| l.name.clone()))
                .collect();
            clip(&names.join(" + "), 40)
        }),
        "update_layer" => {
            layer("id").map(
                |n| match changed_words(&layer_args::patch_from_args(&args["patch"])).as_str() {
                    "" => n,
                    what => format!("{n} · {what}"),
                },
            )
        }
        "move_layer" => layer("id").map(|n| format!("{n} → {}번째", args["index"].as_u64().unwrap_or(0))),
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
        "set_plan" => args["steps"]
            .as_array()
            .map(|steps| format!("{}단계", steps.len())),
        "export_check" => args["dpi"].as_f64().map(|d| format!("{d}dpi")),
        _ => None,
    }
}
