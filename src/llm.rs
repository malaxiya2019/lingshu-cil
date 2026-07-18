use crate::model::{DeltaToolCall, LlmMessage, ModelConfig, StreamChunk, ToolDefinition};
use anyhow::Result;
use std::io::{BufRead, BufReader};
use std::sync::mpsc::{self, Receiver};
use std::thread;

/// Events emitted during streaming LLM completion.
#[derive(Debug)]
pub enum StreamEvent {
    Chunk(String),
    ToolCallDelta(DeltaToolCall),
    Done,
    Error(String),
}

/// Send a streaming completion request with optional tool support.
/// Spawns a background thread for the HTTP call.
pub fn chat_stream(
    config: &ModelConfig,
    messages: &[LlmMessage],
    tools: Option<&[ToolDefinition]>,
) -> Result<Receiver<StreamEvent>> {
    let (tx, rx) = mpsc::channel::<StreamEvent>();

    let model_name = config.name.clone();
    let base_url = config.base_url.clone();
    let api_key = config.api_key.clone()
        .ok_or_else(|| anyhow::anyhow!("API key not set for provider: {}", config.provider))?;
    let max_tokens = config.max_tokens;
    let temperature = config.temperature;
    let messages_json = messages.iter().map(|m| {
        let mut obj = serde_json::json!({
            "role": m.role,
            "content": m.content,
        });
        if let Some(ref tcs) = m.tool_calls {
            obj["tool_calls"] = serde_json::to_value(tcs).unwrap_or_default();
        }
        if let Some(ref tid) = m.tool_call_id {
            obj["tool_call_id"] = serde_json::json!(tid);
        }
        obj
    }).collect::<Vec<_>>();
    let tools_json = tools.map(|t| serde_json::to_value(t).unwrap_or_default());

    thread::spawn(move || {
        let send_err = |msg: String| { let _ = tx.send(StreamEvent::Error(msg)); };
        let send_done = || { let _ = tx.send(StreamEvent::Done); };

        let client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
        {
            Ok(c) => c,
            Err(e) => { send_err(format!("HTTP client: {}", e)); return; }
        };

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let mut body = serde_json::json!({
            "model": model_name,
            "messages": messages_json,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": true,
        });
        if let Some(ref tj) = tools_json {
            body["tools"] = tj.clone();
        }

        let response = match client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
        {
            Ok(r) => r,
            Err(e) => { send_err(format!("HTTP error: {} (url: {})", e, url)); return; }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().unwrap_or_default();
            send_err(format!("API error ({}): {}", status, body_text));
            return;
        }

        // SSE streaming
        let reader = BufReader::new(response);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => { send_err(format!("Stream read: {}", e)); return; }
            };
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(':') { continue; }
            if trimmed == "data: [DONE]" { send_done(); return; }
            if let Some(data) = trimmed.strip_prefix("data: ") {
                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    for choice in &chunk.choices {
                        // Send content chunks
                        if let Some(ref content) = choice.delta.content {
                            if !content.is_empty() && tx.send(StreamEvent::Chunk(content.clone())).is_err() { return; }
                        }
                        // Send tool call deltas
                        if let Some(ref tcs) = choice.delta.tool_calls {
                            for tc in tcs {
                                if tx.send(StreamEvent::ToolCallDelta(tc.clone())).is_err() { return; }
                            }
                        }
                        if choice.finish_reason.is_some() {
                            send_done(); return;
                        }
                    }
                }
            }
        }
        send_done();
    });

    Ok(rx)
}

/// Build the system prompt for the coding agent
pub fn build_system_prompt() -> String {
    r#"You are LingShu CIL, an AI coding assistant integrated into the user's terminal.

You help with software development tasks:
- Code analysis, review, debugging
- File editing and patching
- Shell commands and build tools
- Git operations
- Project architecture discussions

When you need to interact with the user's environment, use the available tools.
Be concise and precise. Always include file paths when referencing code.
When suggesting changes, provide complete file edits or diffs."#.to_string()
}
