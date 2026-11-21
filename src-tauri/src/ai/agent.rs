//! The tool-calling loop, ported from accountanta_ai's agent: send the
//! conversation, run any tools the model asks for (concurrently), feed the
//! results back, and repeat until the model answers in plain text.
//!
//! Differences from the original: fallback models are handled by OpenRouter
//! (`ChatRequest::models`), so a failed model never leaves tool turns in the
//! history; and the loop can be cancelled between and during turns.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use super::openrouter::{ChatRequest, Completion, Message, OpenRouter, Tool, Usage};
use crate::error::{Error, Result};

pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>>;

/// Tools the model can use. Errors are sent back to the model as
/// `{"error": ...}` so it can recover (as in the original agent).
pub trait ToolHost: Send + Sync {
    fn tools(&self) -> Vec<Tool>;
    fn call<'a>(&'a self, name: &'a str, args: Value) -> ToolFuture<'a>;
}

/// Something the UI may want to show while the agent works.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    Text(String),
    ToolStart(String),
    ToolEnd(String),
}

#[derive(Debug, Clone, Default)]
pub struct AgentResult {
    pub text: String,
    pub usage: Usage,
    pub model: String,
    pub tool_calls: u32,
}

/// Run the loop. `request` carries the model, fallbacks and sampling options;
/// its `messages` are the conversation so far.
pub async fn run(
    client: &OpenRouter,
    mut request: ChatRequest,
    host: &dyn ToolHost,
    max_iterations: u32,
    mut on_event: impl FnMut(AgentEvent) + Send,
    keep_going: impl Fn() -> bool + Send + Sync,
) -> Result<AgentResult> {
    request.tools = host.tools();
    if !request.tools.is_empty() {
        request.tool_choice = Some(Value::String("auto".into()));
    }
    let mut result = AgentResult::default();
    for iteration in 0..max_iterations {
        if !keep_going() {
            return Err(Error::Cancelled);
        }
        // On the last turn, insist on an answer instead of more tool calls.
        if iteration + 1 == max_iterations {
            request.tools.clear();
            request.tool_choice = None;
        }
        let completion: Completion =
            client.chat_stream(request.clone(), |t| on_event(AgentEvent::Text(t.to_string())), &keep_going).await?;
        result.usage.add(completion.usage);
        result.model = completion.model.clone();
        let calls = completion.tool_calls().to_vec();
        if calls.is_empty() {
            result.text = completion.text().to_string();
            return Ok(result);
        }
        request.messages.push(completion.message.clone());
        for c in &calls {
            on_event(AgentEvent::ToolStart(c.function.name.clone()));
        }
        let outputs = futures_util::future::join_all(calls.iter().map(|c| async move {
            let args = serde_json::from_str::<Value>(if c.function.arguments.trim().is_empty() { "{}" } else { &c.function.arguments });
            let out = match args {
                Ok(args) => host.call(&c.function.name, args).await,
                Err(e) => Err(Error::other(format!("the arguments were not valid JSON: {e}"))),
            };
            match out {
                Ok(v) => v,
                Err(e) => serde_json::json!({ "error": e.to_string() }),
            }
        }))
        .await;
        for (c, out) in calls.iter().zip(outputs) {
            on_event(AgentEvent::ToolEnd(c.function.name.clone()));
            request.messages.push(Message::tool_result(&c.id, out.to_string()));
            result.tool_calls += 1;
        }
    }
    Err(Error::Ai("the coach kept looking things up without answering; try asking again".into()))
}
