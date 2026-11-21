//! A small OpenRouter client: chat completions (plain, streamed, with tools
//! and structured output), speech-to-text, text-to-speech and the model
//! catalogue. OpenRouter speaks the OpenAI API format, so these are the same
//! requests the `openai` SDK sends with `baseURL = https://openrouter.ai/api/v1`.
//!
//! Model fallback uses OpenRouter's own `models` list: if the first model
//! fails, OpenRouter tries the next one server-side, so a failed attempt never
//! leaves half a conversation behind.

use std::time::Duration;

use base64::Engine as _;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::error::{Error, Result};

pub const BASE_URL: &str = "https://openrouter.ai/api/v1";
/// Identify the app to OpenRouter (shown on its dashboards and rankings).
const REFERER: &str = "https://github.com/Inad-Tech-World/grandie_chess";
const TITLE: &str = "Grandie Chess";
/// Longest a non-streamed request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON text, as the model wrote it (may be invalid).
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "function_type")]
    pub kind: String,
    pub function: FunctionCall,
}

fn function_type() -> String {
    "function".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Message { role: Role::System, content: Some(text.into()), tool_calls: None, tool_call_id: None }
    }
    pub fn user(text: impl Into<String>) -> Self {
        Message { role: Role::User, content: Some(text.into()), tool_calls: None, tool_call_id: None }
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Message { role: Role::Assistant, content: Some(text.into()), tool_calls: None, tool_call_id: None }
    }
    pub fn tool_result(id: impl Into<String>, content: impl Into<String>) -> Self {
        Message { role: Role::Tool, content: Some(content.into()), tool_calls: None, tool_call_id: Some(id.into()) }
    }
}

/// A function the model may call.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    /// JSON schema of the arguments.
    pub parameters: Value,
}

impl Tool {
    pub fn function(name: &str, description: &str, parameters: Value) -> Self {
        Tool { kind: "function", function: FunctionDef { name: name.into(), description: description.into(), parameters } }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ChatRequest {
    pub model: String,
    /// Fallback models, tried by OpenRouter if `model` fails.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// e.g. a JSON schema for structured output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
}

impl ChatRequest {
    /// Ask for JSON matching `schema` (OpenAI-style structured output).
    pub fn json_schema(mut self, name: &str, schema: Value) -> Self {
        self.response_format = Some(serde_json::json!({
            "type": "json_schema",
            "json_schema": { "name": name, "strict": true, "schema": schema }
        }));
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    /// Credits charged, when OpenRouter reports it.
    #[serde(default)]
    pub cost: Option<f64>,
}

impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.cost = match (self.cost, other.cost) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        };
    }
}

/// One finished assistant turn.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    pub message: Message,
    pub finish_reason: Option<String>,
    pub usage: Usage,
    /// The model that actually answered (may be a fallback).
    pub model: String,
}

impl Completion {
    pub fn text(&self) -> &str {
        self.message.content.as_deref().unwrap_or("")
    }

    pub fn tool_calls(&self) -> &[ToolCall] {
        self.message.tool_calls.as_deref().unwrap_or(&[])
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
    #[serde(default)]
    finish_reason: Option<String>,
}

/// A model from the catalogue, for the Settings pickers.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: String,
    pub name: String,
    /// US dollars per input token / unit (as OpenRouter reports it).
    pub prompt_price: Option<f64>,
    pub completion_price: Option<f64>,
    pub context_length: Option<f64>,
}

/// What the saved key may spend.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiKeyInfo {
    pub label: Option<String>,
    /// Credits used so far.
    pub usage: Option<f64>,
    /// Spending limit, if one is set.
    pub limit: Option<f64>,
    pub is_free_tier: bool,
}

#[derive(Clone)]
pub struct OpenRouter {
    http: reqwest::Client,
    key: String,
    base: String,
}

impl OpenRouter {
    pub fn new(http: reqwest::Client, key: String) -> Self {
        OpenRouter { http, key, base: BASE_URL.into() }
    }

    /// For tests against a local server.
    pub fn with_base(http: reqwest::Client, key: String, base: impl Into<String>) -> Self {
        OpenRouter { http, key, base: base.into() }
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.post(format!("{}{path}", self.base)).bearer_auth(&self.key).header("HTTP-Referer", REFERER).header("X-Title", TITLE)
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.get(format!("{}{path}", self.base)).bearer_auth(&self.key).header("HTTP-Referer", REFERER).header("X-Title", TITLE)
    }

    /// One chat completion, waiting for the whole answer.
    pub async fn chat(&self, mut req: ChatRequest) -> Result<Completion> {
        req.stream = false;
        let response = self.post("/chat/completions").timeout(REQUEST_TIMEOUT).json(&req).send().await?;
        let response = check(response).await?;
        let body: Value = response.json().await?;
        if let Some(err) = body.get("error") {
            return Err(api_error(None, err));
        }
        let parsed: ChatResponse = serde_json::from_value(body)?;
        let choice = parsed.choices.into_iter().next().ok_or_else(|| Error::Ai("the model returned no answer".into()))?;
        Ok(Completion {
            message: choice.message,
            finish_reason: choice.finish_reason,
            usage: parsed.usage.unwrap_or_default(),
            model: parsed.model,
        })
    }

    /// A streamed chat completion: `on_text` gets each piece of the answer as
    /// it arrives. Tool calls are assembled and returned at the end. Return
    /// `false` from `keep_going` to stop early.
    pub async fn chat_stream(
        &self,
        mut req: ChatRequest,
        mut on_text: impl FnMut(&str),
        keep_going: impl Fn() -> bool,
    ) -> Result<Completion> {
        req.stream = true;
        let response = self.post("/chat/completions").json(&req).send().await?;
        let response = check(response).await?;
        let mut stream = response.bytes_stream();
        let mut parser = SseParser::default();
        let mut acc = StreamAccumulator::default();
        while let Some(chunk) = stream.next().await {
            if !keep_going() {
                return Err(Error::Cancelled);
            }
            for data in parser.push(&chunk?) {
                if data == "[DONE]" {
                    return Ok(acc.finish());
                }
                let value: Value = match serde_json::from_str(&data) {
                    Ok(v) => v,
                    Err(e) => {
                        log::debug!("ignoring unreadable stream chunk: {e}");
                        continue;
                    }
                };
                if let Some(text) = acc.push(&value)? {
                    on_text(&text);
                }
            }
        }
        Ok(acc.finish())
    }

    /// Speech to text. `wav` is a whole WAV file; `prompt` lists words to
    /// expect (chess vocabulary helps a lot).
    pub async fn transcribe(&self, wav: &[u8], model: &str, prompt: Option<&str>, language: Option<&str>) -> Result<String> {
        let mut body = serde_json::json!({
            "model": model,
            "input_audio": { "data": base64::engine::general_purpose::STANDARD.encode(wav), "format": "wav" },
            "temperature": 0,
        });
        if let Some(p) = prompt {
            body["prompt"] = Value::String(p.into());
        }
        if let Some(l) = language {
            body["language"] = Value::String(l.into());
        }
        let response = self.post("/audio/transcriptions").timeout(REQUEST_TIMEOUT).json(&body).send().await?;
        let value: Value = check(response).await?.json().await?;
        value
            .get("text")
            .and_then(Value::as_str)
            .map(|t| t.trim().to_string())
            .ok_or_else(|| Error::Ai("no transcript in the answer".into()))
    }

    /// Text to speech as MP3 bytes.
    pub async fn speech(&self, text: &str, model: &str, voice: &str, speed: Option<f64>) -> Result<Vec<u8>> {
        let mut body = serde_json::json!({ "model": model, "input": text, "voice": voice, "response_format": "mp3" });
        if let Some(s) = speed.filter(|s| (s - 1.0).abs() > 0.01) {
            body["speed"] = serde_json::json!(s);
        }
        let response = self.post("/audio/speech").timeout(REQUEST_TIMEOUT).json(&body).send().await?;
        Ok(check(response).await?.bytes().await?.to_vec())
    }

    /// The model catalogue, optionally only models with an output modality
    /// ("text", "speech" or "transcription").
    pub async fn models(&self, output_modality: Option<&str>) -> Result<Vec<AiModel>> {
        let path = match output_modality {
            Some(m) => {
                format!("/models?output_modalities={}", m.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect::<String>())
            }
            None => "/models".into(),
        };
        let value: Value = check(self.get(&path).timeout(REQUEST_TIMEOUT).send().await?).await?.json().await?;
        Ok(parse_models(&value))
    }

    /// Voices a speech model offers.
    pub async fn voices(&self, model: &str) -> Result<Vec<String>> {
        let value: Value =
            check(self.get(&format!("/audio/speech/models/{model}")).timeout(REQUEST_TIMEOUT).send().await?).await?.json().await?;
        let model = value.get("data").unwrap_or(&value);
        let voices = model
            .pointer("/output_modalities/0/supported_parameters/voice/values")
            .and_then(Value::as_array)
            .map(|vs| vs.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        Ok(voices)
    }

    /// Check the key and how much it may still spend.
    pub async fn key_info(&self) -> Result<AiKeyInfo> {
        let value: Value = check(self.get("/key").timeout(REQUEST_TIMEOUT).send().await?).await?.json().await?;
        let data = value.get("data").unwrap_or(&value);
        Ok(AiKeyInfo {
            label: data.get("label").and_then(Value::as_str).map(str::to_string),
            usage: data.get("usage").and_then(Value::as_f64),
            limit: data.get("limit").and_then(Value::as_f64),
            is_free_tier: data.get("is_free_tier").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

fn parse_models(value: &Value) -> Vec<AiModel> {
    let price =
        |m: &Value, k: &str| m.pointer(&format!("/pricing/{k}")).and_then(|p| p.as_str().and_then(|s| s.parse().ok()).or(p.as_f64()));
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    Some(AiModel {
                        name: m.get("name").and_then(Value::as_str).unwrap_or(&id).to_string(),
                        prompt_price: price(m, "prompt"),
                        completion_price: price(m, "completion"),
                        context_length: m.get("context_length").and_then(Value::as_f64),
                        id,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Turn an HTTP error status into a typed error with OpenRouter's message.
async fn check(response: reqwest::Response) -> Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body: Value = response.json().await.unwrap_or(Value::Null);
    Err(api_error(Some(status.as_u16()), body.get("error").unwrap_or(&body)))
}

fn api_error(status: Option<u16>, err: &Value) -> Error {
    let code = status.or_else(|| err.get("code").and_then(Value::as_u64).map(|c| c as u16));
    let message = err.get("message").and_then(Value::as_str).unwrap_or("unknown error").to_string();
    match code {
        Some(401) => Error::NoApiKey(format!("OpenRouter didn't accept the API key ({message}). Check it in Settings → AI coach.")),
        Some(402) => Error::AiQuota(format!("Your OpenRouter account is out of credits ({message}).")),
        Some(429) => Error::AiQuota("OpenRouter is limiting requests right now. Wait a moment and try again.".into()),
        Some(c) => Error::Ai(format!("{message} (HTTP {c})")),
        None => Error::Ai(message),
    }
}

/// Splits a server-sent-events byte stream into `data:` payloads. Chunks may
/// end mid-line, so partial lines are kept until the rest arrives.
#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
}

impl SseParser {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            // Lines starting with ':' are comments (OpenRouter sends
            // ": OPENROUTER PROCESSING" keep-alives).
            if let Some(data) = line.strip_prefix("data:") {
                out.push(data.trim_start().to_string());
            }
        }
        out
    }
}

/// Builds the final message from streamed deltas.
#[derive(Default)]
pub struct StreamAccumulator {
    text: String,
    calls: Vec<ToolCall>,
    finish_reason: Option<String>,
    usage: Usage,
    model: String,
}

impl StreamAccumulator {
    /// Take one chunk; returns new answer text, if any.
    pub fn push(&mut self, chunk: &Value) -> Result<Option<String>> {
        if let Some(err) = chunk.get("error") {
            return Err(api_error(None, err));
        }
        if let Some(m) = chunk.get("model").and_then(Value::as_str) {
            self.model = m.to_string();
        }
        if let Some(u) = chunk.get("usage").filter(|u| !u.is_null()) {
            if let Ok(u) = serde_json::from_value::<Usage>(u.clone()) {
                self.usage = u;
            }
        }
        let Some(choice) = chunk.pointer("/choices/0") else { return Ok(None) };
        if let Some(r) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(r.to_string());
        }
        let Some(delta) = choice.get("delta") else { return Ok(None) };
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for c in calls {
                let index = c.get("index").and_then(Value::as_u64).unwrap_or(self.calls.len() as u64) as usize;
                while self.calls.len() <= index {
                    self.calls.push(ToolCall {
                        id: String::new(),
                        kind: function_type(),
                        function: FunctionCall { name: String::new(), arguments: String::new() },
                    });
                }
                let call = &mut self.calls[index];
                if let Some(id) = c.get("id").and_then(Value::as_str) {
                    call.id = id.to_string();
                }
                if let Some(name) = c.pointer("/function/name").and_then(Value::as_str) {
                    call.function.name.push_str(name);
                }
                if let Some(args) = c.pointer("/function/arguments").and_then(Value::as_str) {
                    call.function.arguments.push_str(args);
                }
            }
        }
        match delta.get("content").and_then(Value::as_str) {
            Some(text) if !text.is_empty() => {
                self.text.push_str(text);
                Ok(Some(text.to_string()))
            }
            _ => Ok(None),
        }
    }

    pub fn finish(self) -> Completion {
        let calls: Vec<ToolCall> = self.calls.into_iter().filter(|c| !c.function.name.is_empty()).collect();
        Completion {
            message: Message {
                role: Role::Assistant,
                content: (!self.text.is_empty()).then_some(self.text),
                tool_calls: (!calls.is_empty()).then_some(calls),
                tool_call_id: None,
            },
            finish_reason: self.finish_reason,
            usage: self.usage,
            model: self.model,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_lines_split_across_chunks() {
        let mut p = SseParser::default();
        assert!(p.push(b": OPENROUTER PROCESSING\n\ndata: {\"a\"").is_empty());
        assert_eq!(p.push(b":1}\n\ndata: [DONE]\n"), vec!["{\"a\":1}".to_string(), "[DONE]".to_string()]);
        assert_eq!(p.push(b"data: x\r\n"), vec!["x".to_string()]);
    }

    #[test]
    fn accumulates_text_tool_calls_and_usage() {
        let chunks = [
            r#"{"model":"anthropic/claude-haiku-5.5","choices":[{"delta":{"role":"assistant","content":"Nf3 "}}]}"#,
            r#"{"choices":[{"delta":{"content":"develops."}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"show_line","arguments":"{\"mo"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ves\":[\"e4\"]}"}}]}}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":120,"completion_tokens":30,"cost":0.0004}}"#,
        ];
        let mut acc = StreamAccumulator::default();
        let mut text = String::new();
        for c in chunks {
            if let Some(t) = acc.push(&serde_json::from_str(c).unwrap()).unwrap() {
                text.push_str(&t);
            }
        }
        let done = acc.finish();
        assert_eq!(text, "Nf3 develops.");
        assert_eq!(done.text(), "Nf3 develops.");
        assert_eq!(done.model, "anthropic/claude-haiku-5.5");
        assert_eq!(done.finish_reason.as_deref(), Some("tool_calls"));
        let call = &done.tool_calls()[0];
        assert_eq!(
            (call.id.as_str(), call.function.name.as_str(), call.function.arguments.as_str()),
            ("call_1", "show_line", r#"{"moves":["e4"]}"#)
        );
        assert_eq!(done.usage, Usage { prompt_tokens: 120, completion_tokens: 30, cost: Some(0.0004) });
    }

    #[test]
    fn stream_errors_become_typed_errors() {
        let mut acc = StreamAccumulator::default();
        let err = acc.push(&serde_json::json!({"error": {"code": 402, "message": "Insufficient credits"}})).unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::AiQuota);
        let err = api_error(Some(401), &serde_json::json!({"message": "No auth credentials found"}));
        assert_eq!(err.kind(), crate::error::ErrorKind::NoApiKey);
    }

    #[test]
    fn requests_serialise_like_the_openai_api() {
        let req = ChatRequest {
            model: "anthropic/claude-sonnet-5.5".into(),
            models: vec!["anthropic/claude-haiku-5.5".into()],
            messages: vec![Message::system("You are a chess coach."), Message::user("Why Nf3?")],
            tools: vec![Tool::function("analyse_position", "Evaluate", serde_json::json!({"type": "object", "properties": {}}))],
            temperature: Some(0.4),
            ..Default::default()
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["messages"][0], serde_json::json!({"role": "system", "content": "You are a chess coach."}));
        assert_eq!(v["models"][0], "anthropic/claude-haiku-5.5");
        assert_eq!(v["tools"][0]["type"], "function");
        assert!(v.get("stream").is_none() && v.get("response_format").is_none());
        let with_schema = req.json_schema("narration", serde_json::json!({"type": "object"}));
        assert_eq!(serde_json::to_value(&with_schema).unwrap()["response_format"]["type"], "json_schema");
    }

    #[test]
    fn parses_the_model_catalogue() {
        let v = serde_json::json!({"data": [
            {"id": "anthropic/claude-haiku-5.5", "name": "Claude Haiku 5.5", "context_length": 1000000, "pricing": {"prompt": "0.0000001", "completion": "0.0000005"}},
            {"id": "x/y"}
        ]});
        let models = parse_models(&v);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].prompt_price, Some(0.0000001));
        assert_eq!(models[1].name, "x/y");
    }
}
