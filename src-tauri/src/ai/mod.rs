//! AI through OpenRouter: the HTTP client, the tool-calling agent loop, and
//! local bookkeeping (what was spent, and cached answers so repeating an
//! explanation costs nothing). The API key lives in the system credential
//! store and every call goes through Rust; the web view can't reach the
//! network (see the CSP in tauri.conf.json).

pub mod agent;
pub mod openrouter;
pub mod store;

pub use openrouter::{ChatRequest, Message, OpenRouter};
