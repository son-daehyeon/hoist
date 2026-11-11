use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const BASE: &str = "https://api.openai.com/v1";

#[derive(Clone)]
pub struct OpenAI {
    http: reqwest::Client,
    key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
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
    pub fn answer(&self, name: &str) -> Option<&Value> {
        self.answers.iter().find(|a| a["name"] == name)
    }

    /// The chosen value of a choice question with its probability.
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
            let text = res.text().await.unwrap_or_default();
            let (code, param, message) = error_parts(&text);
            if is_context_overflow(code.as_deref(), &message) {
                return Err(ContextOverflow(format!("OpenAI {status}: {message}")).into());
            }
            match param {
                Some(param) => bail!("OpenAI {status}: {message} (param: {param})"),
                None => bail!("OpenAI {status}: {message}"),
            }
        }
        let mut stream = res.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut final_out: Option<ResponseOut> = None;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("스트림 읽기 실패")?;
            buf.extend_from_slice(&chunk);
            for frame in take_frames(&mut buf) {
                let mut data = String::new();
                for line in frame.lines() {
                    if let Some(d) = line.strip_prefix("data:") {
                        data.push_str(d.trim_start());
                        data.push('\n');
                    }
                }
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(ev) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                let ty = ev["type"].as_str().unwrap_or("");
                match ty {
                    "response.output_text.delta" => {
                        if let Some(d) = ev["delta"].as_str() {
                            on_text(d);
                        }
                    }
                    "response.completed" | "response.incomplete" => {
                        final_out = Some(parse_response(&ev["response"])?);
                    }
                    "response.failed" | "error" => {
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
                            return Err(ContextOverflow(format!("OpenAI: {msg}")).into());
                        }
                        bail!("OpenAI: {msg}");
                    }
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
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            bail!("OpenAI compact {}: {}", status, short_error(&text));
        }
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
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            bail!("OpenAI decisions {}: {}", status, short_error(&text));
        }
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
        let text = if edit {
            let mut form = reqwest::multipart::Form::new()
                .text("model", req.model.clone())
                .text("prompt", req.prompt.clone())
                .text("size", size)
                .text("quality", req.quality.clone())
                .text("output_format", "png")
                .text("n", "1");
            // Opaque when not asked for transparency: an outpainted strip otherwise comes back half transparent
            // and the white under it shows as haze and speckles.
            form = form.text(
                "background",
                if req.transparent { "transparent" } else { "opaque" },
            );
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
            let res = self
                .http
                .post(url)
                .bearer_auth(&self.key)
                .multipart(form)
                .send()
                .await
                .context("이미지 편집 요청 실패")?;
            let status = res.status();
            let text = res.text().await?;
            if !status.is_success() {
                bail!("OpenAI images {}: {}", status, short_error(&text));
            }
            text
        } else {
            let mut body = json!({"model": req.model, "prompt": req.prompt, "size": size, "quality": req.quality, "output_format": "png", "n": 1});
            body["background"] = json!(if req.transparent { "transparent" } else { "opaque" });
            let res = self
                .http
                .post(url)
                .bearer_auth(&self.key)
                .json(&body)
                .send()
                .await
                .context("이미지 생성 요청 실패")?;
            let status = res.status();
            let text = res.text().await?;
            if !status.is_success() {
                bail!("OpenAI images {}: {}", status, short_error(&text));
            }
            text
        };
        let v: Value = serde_json::from_str(&text)?;
        let b64 = v["data"][0]["b64_json"]
            .as_str()
            .ok_or_else(|| anyhow!("이미지 응답에 b64_json이 없어요"))?;
        use base64::Engine;
        let png = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let usage = Usage {
            input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
            cached_input_tokens: v["usage"]["input_tokens_details"]["cached_tokens"]
                .as_u64()
                .unwrap_or(0),
        };
        Ok(ImageOut { png, usage })
    }
}

/// Takes the complete SSE frames (each ended by a blank line) out of `buf` and leaves the unfinished tail.
/// Frames are cut on bytes and decoded whole: decoding every network chunk on its own turned a Korean letter
/// that straddled two chunks into U+FFFD, inside layer names and banner text alike.
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
    let usage = Usage {
        input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
        cached_input_tokens: v["usage"]["input_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0),
    };
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

pub fn text_cost_krw(model: &str, u: &Usage) -> f64 {
    let (i, c, o) = match model {
        m if m.contains("astra") => (10.0, 1.0, 50.0),
        m if m.contains("sol") => (2.0, 0.10, 10.0),
        m if m.contains("terra") => (1.0, 0.05, 5.0),
        m if m.contains("luna") => (0.10, 0.01, 0.50),
        _ => (2.0, 0.2, 10.0),
    };
    let usd = (u.input_tokens.saturating_sub(u.cached_input_tokens)) as f64 * i / 1e6
        + u.cached_input_tokens as f64 * c / 1e6
        + u.output_tokens as f64 * o / 1e6;
    usd * 1400.0
}

pub fn decision_cost_krw(u: &Usage) -> f64 {
    u.input_tokens as f64 * 0.10 / 1e6 * 1400.0
}

pub fn image_cost_krw(u: &Usage) -> f64 {
    let usd = u.input_tokens as f64 * 8.0 / 1e6 + u.output_tokens as f64 * 30.0 / 1e6;
    usd * 1400.0
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
