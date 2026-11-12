use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use futures_util::StreamExt;
use serde_json::{Value, json};

const BASE: &str = "https://api.openai.com/v1";

#[derive(Debug, PartialEq)]
pub enum KeyCheck {
    Valid,
    Rejected,
    Unreachable,
}

pub async fn check_key(key: &str) -> KeyCheck {
    let sent = reqwest::Client::new()
        .get(format!("{BASE}/models"))
        .bearer_auth(key)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    sent.map_or(KeyCheck::Unreachable, |res| key_check_of(res.status()))
}

fn key_check_of(status: reqwest::StatusCode) -> KeyCheck {
    use reqwest::StatusCode;
    // 403: a restricted key without the model-list permission is still a working key.
    match status {
        StatusCode::UNAUTHORIZED => KeyCheck::Rejected,
        s if s.is_success() || s == StatusCode::FORBIDDEN => KeyCheck::Valid,
        _ => KeyCheck::Unreachable,
    }
}

#[derive(Clone)]
pub struct OpenAI {
    http: reqwest::Client,
    key: String,
}

#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct ResponseOut {
    pub output: Vec<Value>,
    pub usage: Usage,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ContextOverflow(pub String);

#[derive(Debug, Clone)]
pub struct ImageRequest {
    pub model: String,
    pub prompt: String,
    pub size: Option<(u32, u32)>,
    pub quality: String,
    pub transparent: bool,
    pub references: Vec<Vec<u8>>,
    pub mask_png: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct ImageOut {
    pub png: Vec<u8>,
    pub usage: Usage,
}

#[derive(Debug, Clone)]
pub struct DecisionOut {
    pub answers: Vec<Value>,
    pub usage: Usage,
}

impl DecisionOut {
    fn answer(&self, name: &str) -> Option<&Value> {
        self.answers.iter().find(|a| a["name"] == name)
    }

    pub fn choice(&self, name: &str) -> Option<(String, f64)> {
        let a = self.answer(name)?;
        let choice = a["choice"].as_str()?.to_string();
        let p = a["probabilities"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["value"] == choice.as_str()))
            .and_then(|p| p["probability"].as_f64())
            .or_else(|| a["confidence"].as_f64())
            .unwrap_or(1.0);
        Some((choice, p))
    }

    pub fn probability(&self, name: &str) -> Option<f64> {
        self.answer(name)?["probability"].as_f64()
    }
}

impl OpenAI {
    pub fn new(key: String) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(20))
            .build()?;
        Ok(OpenAI { http, key })
    }

    pub async fn responses_stream(
        &self,
        mut body: Value,
        mut on_text: impl FnMut(&str),
    ) -> anyhow::Result<ResponseOut> {
        body["stream"] = json!(true);
        let res = self
            .http
            .post(format!("{BASE}/responses"))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .context("responses 요청 실패")?;
        let status = res.status();
        if !status.is_success() {
            return Err(http_failure(status, &res.text().await.unwrap_or_default()));
        }
        let mut stream = res.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut final_out: Option<ResponseOut> = None;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("스트림 읽기 실패")?;
            buf.extend_from_slice(&chunk);
            for frame in take_frames(&mut buf) {
                let Some(ev) = sse_event(&frame) else {
                    continue;
                };
                match ev["type"].as_str().unwrap_or("") {
                    "response.output_text.delta" => {
                        if let Some(d) = ev["delta"].as_str() {
                            on_text(d);
                        }
                    }
                    "response.completed" | "response.incomplete" => {
                        final_out = Some(parse_response(&ev["response"])?);
                    }
                    "response.failed" | "error" => return Err(stream_failure(&ev)),
                    _ => {}
                }
            }
        }
        final_out.ok_or_else(|| anyhow!("응답이 완료 이벤트 없이 끝났어요"))
    }

    pub async fn compact(
        &self,
        model: &str,
        input: &[Value],
        instructions: &str,
    ) -> anyhow::Result<Vec<Value>> {
        let body = json!({"model": model, "input": input, "instructions": instructions});
        let res = self
            .http
            .post(format!("{BASE}/responses/compact"))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .context("compact 요청 실패")?;
        let text = body_or_error(res, "compact").await?;
        let v: Value = serde_json::from_str(&text)?;
        Ok(v["output"].as_array().cloned().unwrap_or_default())
    }

    /// `input` is a string or a list of messages (`input_text`/`input_image` parts); `questions` are
    /// `choice`, `predicate` or `score` questions. Output tokens are free, so only the input is billed.
    pub async fn decide(
        &self,
        model: &str,
        input: Value,
        questions: &[Value],
        timeout: Duration,
    ) -> anyhow::Result<DecisionOut> {
        let body = json!({"model": model, "input": input, "questions": questions});
        let res = self
            .http
            .post(format!("{BASE}/decisions"))
            .timeout(timeout)
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .context("decisions 요청 실패")?;
        let text = body_or_error(res, "decisions").await?;
        let v: Value = serde_json::from_str(&text)?;
        Ok(DecisionOut {
            answers: v["answers"].as_array().cloned().unwrap_or_default(),
            usage: Usage {
                input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
                output_tokens: 0,
                cached_input_tokens: 0,
            },
        })
    }

    pub async fn image(&self, req: ImageRequest) -> anyhow::Result<ImageOut> {
        let edit = !req.references.is_empty();
        let url = if edit {
            format!("{BASE}/images/edits")
        } else {
            format!("{BASE}/images/generations")
        };
        let size = req
            .size
            .map(|(w, h)| format!("{w}x{h}"))
            .unwrap_or_else(|| "auto".into());
        // Opaque when not asked for transparency: an outpainted strip otherwise comes back half transparent
        // and the white under it shows as haze and speckles.
        let background = if req.transparent { "transparent" } else { "opaque" };
        let res = if edit {
            let mut form = reqwest::multipart::Form::new()
                .text("model", req.model.clone())
                .text("prompt", req.prompt.clone())
                .text("size", size)
                .text("quality", req.quality.clone())
                .text("output_format", "png")
                .text("n", "1")
                .text("background", background);
            for (i, bytes) in req.references.iter().enumerate() {
                let part = reqwest::multipart::Part::bytes(bytes.clone())
                    .file_name(format!("ref{i}.png"))
                    .mime_str("image/png")?;
                form = form.part("image[]", part);
            }
            if let Some(mask) = req.mask_png {
                let part = reqwest::multipart::Part::bytes(mask)
                    .file_name("mask.png")
                    .mime_str("image/png")?;
                form = form.part("mask", part);
            }
            self.http
                .post(url)
                .bearer_auth(&self.key)
                .multipart(form)
                .send()
                .await
                .context("이미지 편집 요청 실패")?
        } else {
            let body = json!({"model": req.model, "prompt": req.prompt, "size": size, "quality": req.quality, "output_format": "png", "n": 1, "background": background});
            self.http
                .post(url)
                .bearer_auth(&self.key)
                .json(&body)
                .send()
                .await
                .context("이미지 생성 요청 실패")?
        };
        let text = body_or_error(res, "images").await?;
        let v: Value = serde_json::from_str(&text)?;
        let b64 = v["data"][0]["b64_json"]
            .as_str()
            .ok_or_else(|| anyhow!("이미지 응답에 b64_json이 없어요"))?;
        use base64::Engine;
        let png = base64::engine::general_purpose::STANDARD.decode(b64)?;
        Ok(ImageOut {
            png,
            usage: usage_of(&v),
        })
    }
}

async fn body_or_error(res: reqwest::Response, api: &str) -> anyhow::Result<String> {
    let status = res.status();
    let text = res.text().await?;
    if !status.is_success() {
        bail!("OpenAI {api} {status}: {}", short_error(&text));
    }
    Ok(text)
}

fn http_failure(status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let (code, param, message) = error_parts(body);
    if is_context_overflow(code.as_deref(), &message) {
        return ContextOverflow(format!("OpenAI {status}: {message}")).into();
    }
    match param {
        Some(param) => anyhow!("OpenAI {status}: {message} (param: {param})"),
        None => anyhow!("OpenAI {status}: {message}"),
    }
}

fn sse_event(frame: &str) -> Option<Value> {
    let mut data = String::new();
    for line in frame.lines() {
        if let Some(d) = line.strip_prefix("data:") {
            data.push_str(d.trim_start());
            data.push('\n');
        }
    }
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    serde_json::from_str(data).ok()
}

fn stream_failure(ev: &Value) -> anyhow::Error {
    let msg = ev["response"]["error"]["message"]
        .as_str()
        .or_else(|| ev["error"]["message"].as_str())
        .or_else(|| ev["message"].as_str())
        .unwrap_or("알 수 없는 오류");
    let code = ev["response"]["error"]["code"]
        .as_str()
        .or_else(|| ev["error"]["code"].as_str())
        .or_else(|| ev["code"].as_str());
    if is_context_overflow(code, msg) {
        return ContextOverflow(format!("OpenAI: {msg}")).into();
    }
    anyhow!("OpenAI: {msg}")
}

fn usage_of(v: &Value) -> Usage {
    Usage {
        input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
        cached_input_tokens: v["usage"]["input_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0),
    }
}

/// Frames are cut on bytes and decoded whole: a multibyte letter can straddle two network chunks.
fn take_frames(buf: &mut Vec<u8>) -> Vec<String> {
    let mut frames = Vec::new();
    while let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
        frames.push(String::from_utf8_lossy(&buf[..pos]).into_owned());
        buf.drain(..pos + 2);
    }
    frames
}

fn parse_response(v: &Value) -> anyhow::Result<ResponseOut> {
    let output = v["output"].as_array().cloned().unwrap_or_default();
    let usage = usage_of(v);
    if v["status"].as_str() == Some("failed") {
        bail!(
            "OpenAI: {}",
            v["error"]["message"].as_str().unwrap_or("응답 실패")
        );
    }
    Ok(ResponseOut { output, usage })
}

fn error_parts(text: &str) -> (Option<String>, Option<String>, String) {
    let v = serde_json::from_str::<Value>(text).ok();
    let field = |key: &str| {
        v.as_ref()
            .and_then(|v| v["error"][key].as_str().map(str::to_string))
    };
    (field("code"), field("param"), short_error(text))
}

fn is_context_overflow(code: Option<&str>, message: &str) -> bool {
    code == Some("context_length_exceeded")
        || message.contains("maximum context length")
        || message.contains("context window")
}

fn short_error(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| text.chars().take(300).collect())
}

const KRW_PER_USD: f64 = 1400.0;

pub fn text_cost_krw(model: &str, usage: &Usage) -> f64 {
    let (input_rate, cached_rate, output_rate) = match model {
        m if m.contains("astra") => (10.0, 1.0, 50.0),
        m if m.contains("sol") => (2.0, 0.10, 10.0),
        m if m.contains("terra") => (1.0, 0.05, 5.0),
        m if m.contains("luna") => (0.10, 0.01, 0.50),
        _ => (2.0, 0.2, 10.0),
    };
    let usd = (usage.input_tokens.saturating_sub(usage.cached_input_tokens)) as f64 * input_rate / 1e6
        + usage.cached_input_tokens as f64 * cached_rate / 1e6
        + usage.output_tokens as f64 * output_rate / 1e6;
    usd * KRW_PER_USD
}

pub fn decision_cost_krw(usage: &Usage) -> f64 {
    usage.input_tokens as f64 * 0.10 / 1e6 * KRW_PER_USD
}

pub fn image_cost_krw(usage: &Usage) -> f64 {
    let usd = usage.input_tokens as f64 * 8.0 / 1e6 + usage.output_tokens as f64 * 30.0 / 1e6;
    usd * KRW_PER_USD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_401_rejects_a_key() {
        use reqwest::StatusCode;
        assert_eq!(key_check_of(StatusCode::OK), KeyCheck::Valid);
        assert_eq!(key_check_of(StatusCode::FORBIDDEN), KeyCheck::Valid);
        assert_eq!(key_check_of(StatusCode::UNAUTHORIZED), KeyCheck::Rejected);
        assert_eq!(key_check_of(StatusCode::TOO_MANY_REQUESTS), KeyCheck::Unreachable);
        assert_eq!(key_check_of(StatusCode::BAD_GATEWAY), KeyCheck::Unreachable);
    }

    #[test]
    fn a_letter_split_between_chunks_is_not_broken() {
        let stream = "data: {\"delta\":\"표어 오른쪽 잎\"}\n\ndata: {\"delta\":\"맞아요\"}\n\n".as_bytes();
        for cut in 1..stream.len() {
            let mut buf = Vec::new();
            buf.extend_from_slice(&stream[..cut]);
            let mut frames = take_frames(&mut buf);
            buf.extend_from_slice(&stream[cut..]);
            frames.extend(take_frames(&mut buf));
            assert_eq!(
                frames,
                [
                    "data: {\"delta\":\"표어 오른쪽 잎\"}",
                    "data: {\"delta\":\"맞아요\"}"
                ],
                "cut at byte {cut}"
            );
            assert!(buf.is_empty());
        }
    }

    #[test]
    fn an_sse_frame_gives_the_json_of_its_data_lines() {
        assert_eq!(sse_event("data: {\"a\":1}"), Some(json!({"a": 1})));
        assert_eq!(sse_event("event: x\ndata: {\"a\":1}"), Some(json!({"a": 1})));
        for frame in ["data:[DONE]", "", "data: {broken"] {
            assert_eq!(sse_event(frame), None, "{frame:?}");
        }
    }

    #[test]
    fn a_stream_failure_keeps_its_message_and_marks_a_context_overflow() {
        for ev in [
            json!({"response": {"error": {"message": "boom"}}}),
            json!({"error": {"message": "boom"}}),
            json!({"message": "boom"}),
        ] {
            let failure = stream_failure(&ev);
            assert_eq!(failure.to_string(), "OpenAI: boom");
            assert!(failure.downcast_ref::<ContextOverflow>().is_none());
        }
        assert_eq!(stream_failure(&json!({})).to_string(), "OpenAI: 알 수 없는 오류");
        let overflow =
            stream_failure(&json!({"error": {"message": "too long", "code": "context_length_exceeded"}}));
        assert_eq!(overflow.to_string(), "OpenAI: too long");
        assert!(overflow.downcast_ref::<ContextOverflow>().is_some());
    }

    #[test]
    fn an_http_failure_names_the_parameter_and_marks_a_context_overflow() {
        let status = reqwest::StatusCode::BAD_REQUEST;
        let failure = http_failure(status, r#"{"error":{"message":"bad value","param":"x"}}"#);
        assert_eq!(
            failure.to_string(),
            "OpenAI 400 Bad Request: bad value (param: x)"
        );
        assert!(failure.downcast_ref::<ContextOverflow>().is_none());
        assert_eq!(
            http_failure(status, "not json").to_string(),
            "OpenAI 400 Bad Request: not json"
        );
        let overflow = http_failure(
            status,
            r#"{"error":{"message":"over the maximum context length"}}"#,
        );
        assert_eq!(
            overflow.to_string(),
            "OpenAI 400 Bad Request: over the maximum context length"
        );
        assert!(overflow.downcast_ref::<ContextOverflow>().is_some());
    }

    #[test]
    fn the_decision_answer_is_read_with_its_probability() {
        let out = DecisionOut {
            answers: vec![
                json!({"type": "choice", "name": "kind", "choice": "simple", "probabilities": [{"value": "simple", "probability": 0.91}, {"value": "normal", "probability": 0.09}], "confidence": 0.5}),
            ],
            usage: Usage::default(),
        };
        assert_eq!(out.choice("kind"), Some(("simple".to_string(), 0.91)));
        assert_eq!(out.choice("other"), None);
        let out = DecisionOut {
            answers: vec![json!({"type": "predicate", "name": "crowded", "probability": 0.8})],
            usage: Usage::default(),
        };
        assert_eq!(out.probability("crowded"), Some(0.8));
    }
}
