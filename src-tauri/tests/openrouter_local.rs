//! The OpenRouter client against a local HTTP server with canned answers:
//! streaming with tool calls, plain chat, speech-to-text, text-to-speech and
//! error mapping. No network or key needed.

use std::sync::{Arc, Mutex};

use grandie_chess_lib::ai::openrouter::{ChatRequest, Message, OpenRouter};
use grandie_chess_lib::error::ErrorKind;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Serve one canned response per connection, recording request bodies.
async fn server(responses: Vec<(u16, &'static str, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    tokio::spawn(async move {
        for (status, content_type, body) in responses {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 1 << 20];
            let mut got = 0;
            // Read headers, then the body by Content-Length.
            loop {
                let n = sock.read(&mut buf[got..]).await.unwrap();
                got += n;
                let text = String::from_utf8_lossy(&buf[..got]).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len: usize = text
                        .lines()
                        .find_map(|l| l.to_lowercase().strip_prefix("content-length: ").map(|v| v.trim().parse().unwrap_or(0)))
                        .unwrap_or(0);
                    if got >= end + 4 + len {
                        seen2.lock().unwrap().push(text);
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let head = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            sock.write_all(head.as_bytes()).await.unwrap();
            sock.write_all(body.as_bytes()).await.unwrap();
            sock.shutdown().await.ok();
        }
    });
    (format!("http://{addr}"), seen)
}

fn client(base: &str) -> OpenRouter {
    OpenRouter::with_base(reqwest::Client::new(), "sk-or-test".into(), base)
}

#[tokio::test]
async fn streams_text_and_tool_calls() {
    let sse = [
        ": OPENROUTER PROCESSING",
        r#"data: {"model":"anthropic/claude-haiku-5.5","choices":[{"delta":{"content":"Let me check. "}}]}"#,
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"analyse_position","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        "data: [DONE]",
        "",
    ]
    .join("\n\n");
    let (base, seen) = server(vec![(200, "text/event-stream", sse)]).await;
    let mut text = String::new();
    let req = ChatRequest { model: "anthropic/claude-haiku-5.5".into(), messages: vec![Message::user("hi")], ..Default::default() };
    let done = client(&base).chat_stream(req, |t| text.push_str(t), || true).await.unwrap();
    assert_eq!(text, "Let me check. ");
    assert_eq!(done.tool_calls()[0].function.name, "analyse_position");
    assert_eq!(done.usage.prompt_tokens, 10);
    let request = seen.lock().unwrap()[0].clone();
    assert!(request.starts_with("POST /chat/completions"));
    assert!(request.contains("authorization: Bearer sk-or-test") || request.contains("Authorization: Bearer sk-or-test"));
    assert!(request.to_lowercase().contains("x-title: grandie chess"));
    assert!(request.contains(r#""stream":true"#));
}

#[tokio::test]
async fn chat_transcribe_and_speech() {
    let (base, seen) = server(vec![
        (200, "application/json", r#"{"model":"m","choices":[{"message":{"role":"assistant","content":"{\"lines\":[]}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"cost":0.0001}}"#.into()),
        (200, "application/json", r#"{"text":" knight f3 ","usage":{"seconds":1}}"#.into()),
        (200, "audio/mpeg", "ID3fakeaudio".into()),
    ])
    .await;
    let c = client(&base);
    let done = c.chat(ChatRequest { model: "m".into(), messages: vec![Message::user("x")], ..Default::default() }).await.unwrap();
    assert_eq!(done.text(), r#"{"lines":[]}"#);
    assert_eq!(done.usage.cost, Some(0.0001));
    assert_eq!(c.transcribe(b"RIFF....", "openai/gpt-4o-mini-transcribe", Some("chess"), Some("en")).await.unwrap(), "knight f3");
    assert_eq!(c.speech("Nf3", "mistralai/voxtral-mini-tts-3-0", "en_paul_confident", Some(1.0)).await.unwrap(), b"ID3fakeaudio");
    let reqs = seen.lock().unwrap().clone();
    assert!(
        reqs[1].starts_with("POST /audio/transcriptions")
            && reqs[1].contains(r#""format":"wav""#)
            && reqs[1].contains(r#""language":"en""#)
    );
    assert!(reqs[2].starts_with("POST /audio/speech") && reqs[2].contains(r#""response_format":"mp3""#));
}

#[tokio::test]
async fn http_errors_become_typed_errors() {
    let (base, _) = server(vec![
        (401, "application/json", r#"{"error":{"code":401,"message":"No auth credentials found"}}"#.into()),
        (402, "application/json", r#"{"error":{"code":402,"message":"Insufficient credits"}}"#.into()),
    ])
    .await;
    let c = client(&base);
    let req = || ChatRequest { model: "m".into(), messages: vec![Message::user("x")], ..Default::default() };
    assert_eq!(c.chat(req()).await.unwrap_err().kind(), ErrorKind::NoApiKey);
    assert_eq!(c.chat(req()).await.unwrap_err().kind(), ErrorKind::AiQuota);
}
