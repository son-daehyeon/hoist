use std::sync::atomic::Ordering;

use anyhow::{anyhow, bail};
use hoist_core::image as cimg;
use hoist_core::model::FitMode;
use image::RgbaImage;
use serde_json::{Value, json};

use super::{ToolCtx, ToolResult, arg_rect_px};
use crate::agent::{AUTO_ANSWER, AUTO_LABEL, Question, img, prompt};
use crate::state::lock;

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
            let typing = options.is_empty() || q["allow_free_text"].as_bool().unwrap_or(false);
            // With a typing field, a "let me fix it" choice only duplicates the field.
            let mut options = options;
            if typing && options.len() > 1 {
                options.retain(|o| !["수정", "고칠", "고쳐", "다르", "아니"].iter().any(|k| o.contains(k)));
            }
            if (options.len() == 1 && !typing) || options.len() > 4 {
                bail!("options는 2~4개로 주거나(자유 입력을 허용하면 1개도 됩니다), 자유 입력만 받으려면 비워 두세요");
            }
            // The fit question is a real choice for the user, so it leads with 재배치 and has no auto choice.
            if is_fit_question(&options) {
                options.sort_by_key(|o| fit_mode_from_answer(o) != Some(FitMode::Relayout));
            }
            let rect_px = if q["rect_px"].is_null() {
                None
            } else {
                Some(arg_rect_px(q, "rect_px")?)
            };
            Ok(Ask {
                text,
                allow_free_text: typing,
                options,
                rect_px,
            })
        })
        .collect()
}

fn is_fit_question(options: &[String]) -> bool {
    options.len() == 2 && options.iter().all(|o| fit_mode_from_answer(o).is_some())
}

const QUESTION_THUMB_PX: u32 = 512;

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

pub(super) async fn ask_user(ctx: &ToolCtx, args: &Value) -> anyhow::Result<ToolResult> {
    let asks = parse_questions(args)?;
    let original = match (ctx.facts.as_ref(), asks.iter().any(|a| a.rect_px.is_some())) {
        (Some(f), true) => Some(img::original_image(&lock(&ctx.handle), f.version)?),
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
            auto_label: if is_fit_question(&a.options) {
                String::new()
            } else {
                AUTO_LABEL.into()
            },
        };
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        lock(&ctx.run.pending).insert(question.id.clone(), tx);
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
            lock(&ctx.handle).set_fit_mode(mode)?;
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
    use std::sync::Arc;

    use super::*;
    use crate::agent::tools::tests::test_ctx;

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

        let check = parse_questions(&json!({"questions": [
            {"text": "전화가 043-646-6665 맞나요?", "options": ["맞아요", "수정할게요"], "allow_free_text": true},
            {"text": "날짜가 맞나요?", "options": ["맞아요"], "allow_free_text": true},
            {"text": "어떻게 맞출까요?", "options": ["배경만 늘려 채우기(원본 배치 그대로)", "재배치(배치를 바꿔 가독성을 높임)"]}
        ]}))
        .expect("read checks and the fit question");
        assert_eq!(check[0].options, ["맞아요"], "typing replaces the fix-it choice");
        assert_eq!(check[1].options, ["맞아요"]);
        assert!(check[1].allow_free_text);
        assert!(
            check[2].options[0].starts_with("재배치"),
            "the fit question leads with 재배치"
        );
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
        let mut ctx = test_ctx(std::env::temp_dir().join("hoist-no-models"));
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
}
