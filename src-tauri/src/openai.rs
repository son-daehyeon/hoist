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
        let mut buf = String::new();
        let mut final_out: Option<ResponseOut> = None;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("스트림 읽기 실패")?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buf.find("\n\n") {
                let frame = buf[..pos].to_string();
                buf.drain(..pos + 2);
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
            if req.transparent {
                form = form.text("background", "transparent");
            }
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
            if req.transparent {
                body["background"] = json!("transparent");
            }
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

pub fn image_cost_krw(u: &Usage) -> f64 {
    let usd = u.input_tokens as f64 * 8.0 / 1e6 + u.output_tokens as f64 * 30.0 / 1e6;
    usd * 1400.0
}
