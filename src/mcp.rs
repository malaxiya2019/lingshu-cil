//! MCP Server — Model Context Protocol adapter (JSON-RPC 2.0 over stdio)
//!
//! Pure protocol adapter. No business logic, no agent logic, no file ops.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

// ── JSON-RPC 2.0 Types ──

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// ── MCP Protocol Types ──

#[derive(Debug, Serialize)]
struct McpTool {
    name: String,
    description: String,
    input_schema: Value,
}

// ── MCP Server ──

pub struct McpServer;

impl McpServer {
    pub fn new() -> Self {
        Self
    }

    /// Run the MCP server: reads JSON-RPC from stdin, writes to stdout
    pub fn run(&self) -> anyhow::Result<()> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut stdout_lock = stdout.lock();

        eprintln!("[lingshu-cil:mcp] MCP server started on stdio transport");

        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[lingshu-cil:mcp] stdin error: {}", e);
                    break;
                }
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {}", e),
                            data: None,
                        }),
                    };
                    let _ = writeln!(stdout_lock, "{}", serde_json::to_string(&err_resp).unwrap());
                    continue;
                }
            };

            let response = self.handle_request(&request);

            if let Some(resp) = response {
                let _ = writeln!(stdout_lock, "{}", serde_json::to_string(&resp).unwrap());
            }
        }

        Ok(())
    }

    fn handle_request(&self, req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = req.id.clone();
        let method = req.method.as_str();

        Some(match method {
            // ── Initialization ──
            "initialize" => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "protocolVersion": "2024-11-05",
                    "serverInfo": { "name": "lingshu-cil", "version": "0.3.0" },
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "resources": { "listChanged": false }
                    }
                })),
                error: None,
            },

            // ── Resources ──
            "resources/list" => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "resources": [{
                        "uri": "lingshu://models",
                        "name": "Model Catalog",
                        "description": "Available LLM models",
                        "mimeType": "application/json"
                    }]
                })),
                error: None,
            },

            "resources/read" => {
                let uri = req.params.as_ref()
                    .and_then(|p| p.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");

                let catalog = json!({
                    "models": [
                        {"id": "deepseek-chat", "provider": "deepseek", "context_window": 128000},
                        {"id": "deepseek-coder", "provider": "deepseek", "context_window": 128000},
                        {"id": "gpt-4o", "provider": "openai", "context_window": 128000},
                        {"id": "qwen-plus", "provider": "qwen", "context_window": 131072},
                    ]
                });

                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": serde_json::to_string_pretty(&catalog).unwrap()
                        }]
                    })),
                    error: None,
                }
            }

            // ── Tools ──
            "tools/list" => {
                let tools = vec![
                    McpTool {
                        name: "chat".to_string(),
                        description: "Send a chat completion to the LLM API".to_string(),
                        input_schema: json!({
                            "type": "object",
                            "properties": {
                                "model": {"type": "string"},
                                "messages": {"type": "array"},
                                "max_tokens": {"type": "integer", "default": 4096},
                                "temperature": {"type": "number", "default": 0.7}
                            },
                            "required": ["model", "messages"]
                        }),
                    },
                ];

                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({ "tools": tools })),
                    error: None,
                }
            }

            "tools/call" => {
                let tool_name = req.params.as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");

                match tool_name {
                    "chat" => {
                        let args = req.params.as_ref()
                            .and_then(|p| p.get("arguments"))
                            .cloned()
                            .unwrap_or(json!({}));

                        let api_key = std::env::var("DEEPSEEK_API_KEY").unwrap_or_default();
                        if api_key.is_empty() {
                            return Some(JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id,
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32000,
                                    message: "DEEPSEEK_API_KEY not set".to_string(),
                                    data: None,
                                }),
                            });
                        }

                        let model = args.get("model").and_then(|m| m.as_str()).unwrap_or("deepseek-chat");
                        let messages = args.get("messages").and_then(|m| m.as_array()).cloned().unwrap_or_default();
                        let max_tokens = args.get("max_tokens").and_then(|m| m.as_u64()).unwrap_or(4096);
                        let temperature = args.get("temperature").and_then(|t| t.as_f64()).unwrap_or(0.7);

                        match call_llm_api(model, &messages, max_tokens as u32, temperature as f32, &api_key) {
                            Ok(response_text) => JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id,
                                result: Some(json!({
                                    "content": [{"type": "text", "text": response_text}]
                                })),
                                error: None,
                            },
                            Err(e) => JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id,
                                result: None,
                                error: Some(JsonRpcError { code: -32000, message: e, data: None }),
                            },
                        }
                    }
                    _ => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: format!("Unknown tool: {}", tool_name),
                            data: None,
                        }),
                    },
                }
            }

            // ── Ping ──
            "ping" => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({})),
                error: None,
            },

            // ── Notifications (no response) ──
            "notifications/initialized" => return None,

            _ => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", method),
                    data: None,
                }),
            },
        })
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Make an LLM API chat completion call (non-streaming)
fn call_llm_api(model: &str, messages: &[Value], max_tokens: u32, temperature: f32, api_key: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client: {}", e))?;

    let body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": false,
    });

    let response = client
        .post("https://api.deepseek.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("API error ({}): {}", status, body));
    }

    let result: Value = response.json().map_err(|e| format!("Parse error: {}", e))?;
    let content = result["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("No content in response")
        .to_string();

    Ok(content)
}
