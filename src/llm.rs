//! LLM API client — real HTTP streaming chat completions
//!
//! Supports OpenAI-compatible APIs: DeepSeek, OpenAI, Qwen (DashScope)
//! Uses blocking reqwest with thread-based streaming.

use crate::model::{ModelConfig, StreamChunk};
use anyhow::Result;
use std::sync::mpsc::{self, Receiver};
use std::thread;

/// A streaming chat completion result
pub enum StreamEvent {
    Chunk(String),
    Done,
    Error(String),
}

/// Send a streaming chat completion request.
/// Returns a Receiver that yields StreamEvent items as they arrive.
pub fn chat_stream(
    config: &ModelConfig,
    messages: &[ChatMessage],
    system_prompt: Option<&str>,
) -> Result<Receiver<StreamEvent>> {
    let (tx, rx) = mpsc::channel::<StreamEvent>();

    let model_name = config.name.clone();
    let base_url = config.base_url.clone();
    let api_key = config
        .api_key
        .clone()
        .ok_or_else(|| anyhow::anyhow!("API key not set for provider: {}", config.provider))?;
    let max_tokens = config.max_tokens;
    let temperature = config.temperature;

    // Build the messages payload
    let mut api_messages: Vec<serde_json::Value> = Vec::new();

    if let Some(sys) = system_prompt {
        api_messages.push(serde_json::json!({
            "role": "system",
            "content": sys
        }));
    }

    for msg in messages {
        api_messages.push(serde_json::json!({
            "role": msg.role,
            "content": msg.content
        }));
    }

    // Spawn a thread for the blocking HTTP call
    thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(format!("Failed to create HTTP client: {}", e)));
                return;
            }
        };

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

        let request_body = serde_json::json!({
            "model": model_name,
            "messages": api_messages,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": true,
        });

        let response = match client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(format!(
                    "HTTP request failed: {} (url: {})",
                    e, url
                )));
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = match response.text() {
                Ok(b) => b,
                Err(_) => "could not read body".to_string(),
            };
            let _ = tx.send(StreamEvent::Error(format!(
                "API error ({}): {}",
                status, body
            )));
            return;
        }

        // Read the streaming response
        match response.bytes() {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if line == "data: [DONE]" {
                        break;
                    }
                    if let Some(data) = line.strip_prefix("data: ") {
                        match serde_json::from_str::<StreamChunk>(data) {
                            Ok(chunk) => {
                                for choice in &chunk.choices {
                                    if let Some(content) = &choice.delta.content {
                                        if !content.is_empty() {
                                            if tx.send(StreamEvent::Chunk(content.clone())).is_err()
                                            {
                                                return; // receiver dropped
                                            }
                                        }
                                    }
                                    if choice.finish_reason.is_some() {
                                        let _ = tx.send(StreamEvent::Done);
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                // Some SSE lines may not be valid JSON (e.g., "data: [DONE]")
                                eprintln!("[llm] parse error: {} for line: {}", e, data.chars().take(80).collect::<String>());
                            }
                        }
                    }
                }
                let _ = tx.send(StreamEvent::Done);
            }
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(format!(
                    "Failed to read response body: {}",
                    e
                )));
            }
        }
    });

    Ok(rx)
}

/// A message in the chat history
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
        }
    }
}

/// Build a system prompt from workspace context
pub fn build_system_prompt(workspace_summary: &str) -> String {
    format!(
        r#"You are LingShu CIL (Context-aware Interactive LLM), an AI assistant integrated into the user's terminal.

Current workspace context:
{}

You can help with:
- Code analysis, review, and debugging
- Architecture and design discussions
- Security auditing
- Build and deployment optimization
- General technical questions

Be concise but thorough. When referencing code, include file paths."#,
        workspace_summary
    )
}
