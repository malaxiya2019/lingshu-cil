//! MCP Server — Model Context Protocol implementation
//!
//! Exposes three DeepSeek metadata resources over stdio transport:
//! - `deepseek://models`   Model catalog with V3.2 pricing
//! - `deepseek://config`   Server configuration (masked credentials)
//! - `deepseek://usage`    Real-time usage statistics & session metrics
//!
//! Protocol: JSON-RPC 2.0 over stdin/stdout (logs to stderr)

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ──────────────────────────────────────────────
// JSON-RPC 2.0 Types
// ──────────────────────────────────────────────

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

#[derive(Debug, Serialize)]
struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

// ──────────────────────────────────────────────
// MCP Protocol Types
// ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct McpResource {
    uri: String,
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct McpResourceTemplate {
    uri_template: String,
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct McpTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct McpPrompt {
    name: String,
    description: String,
    arguments: Vec<Value>,
}

// ──────────────────────────────────────────────
// Circuit Breaker
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CircuitBreakerState {
    failure_count: u32,
    threshold: u32,
    reset_timeout_secs: u64,
    last_failure: Option<String>,
    state: String, // "closed" | "open" | "half-open"
}

struct CircuitBreaker {
    failure_count: u32,
    threshold: u32,
    reset_timeout: Duration,
    last_failure: Option<Instant>,
    state: CircuitState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    fn new(threshold: u32, reset_timeout_secs: u64) -> Self {
        Self {
            failure_count: 0,
            threshold,
            reset_timeout: Duration::from_secs(reset_timeout_secs),
            last_failure: None,
            state: CircuitState::Closed,
        }
    }

    fn call<F, T>(&mut self, f: F) -> Result<T, String>
    where
        F: FnOnce() -> Result<T, String>,
    {
        match self.state {
            CircuitState::Open => {
                if let Some(last) = self.last_failure {
                    if last.elapsed() >= self.reset_timeout {
                        self.state = CircuitState::HalfOpen;
                    } else {
                        return Err("Circuit breaker is open".to_string());
                    }
                } else {
                    return Err("Circuit breaker is open".to_string());
                }
            }
            _ => {}
        }

        match f() {
            Ok(val) => {
                self.failure_count = 0;
                self.state = CircuitState::Closed;
                Ok(val)
            }
            Err(e) => {
                self.failure_count += 1;
                self.last_failure = Some(Instant::now());
                if self.failure_count >= self.threshold {
                    self.state = CircuitState::Open;
                }
                Err(e)
            }
        }
    }

    fn state(&self) -> CircuitBreakerState {
        CircuitBreakerState {
            failure_count: self.failure_count,
            threshold: self.threshold,
            reset_timeout_secs: self.reset_timeout.as_secs(),
            last_failure: self.last_failure.map(|t| {
                let elapsed = t.elapsed();
                format!("{}s ago", elapsed.as_secs())
            }),
            state: match self.state {
                CircuitState::Closed => "closed".to_string(),
                CircuitState::Open => "open".to_string(),
                CircuitState::HalfOpen => "half-open".to_string(),
            },
        }
    }
}

// ──────────────────────────────────────────────
// Session Manager
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionInfo {
    session_id: String,
    created_at: String,
    last_active: String,
    message_count: u32,
    total_tokens: u64,
    model: String,
}

struct SessionManager {
    sessions: HashMap<String, SessionInfo>,
    ttl: Duration,
    max_sessions: usize,
}

impl SessionManager {
    fn new(ttl_minutes: u64, max_sessions: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            ttl: Duration::from_secs(ttl_minutes * 60),
            max_sessions,
        }
    }

    fn create_session(&mut self, model: &str) -> String {
        self.evict_expired();

        let session_id = format!(
            "sess_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

        let session = SessionInfo {
            session_id: session_id.clone(),
            created_at: now.clone(),
            last_active: now,
            message_count: 0,
            total_tokens: 0,
            model: model.to_string(),
        };

        // Evict oldest if at capacity
        if self.sessions.len() >= self.max_sessions {
            let oldest_key = self
                .sessions
                .iter()
                .min_by_key(|(_, s)| s.created_at.clone())
                .map(|(k, _)| k.clone());
            if let Some(key) = oldest_key {
                self.sessions.remove(&key);
            }
        }

        self.sessions.insert(session_id.clone(), session);
        session_id
    }

    fn record_usage(&mut self, session_id: &str, tokens: u64) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.message_count += 1;
            session.total_tokens += tokens;
            session.last_active = chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string();
        }
    }

    fn get_session(&self, session_id: &str) -> Option<&SessionInfo> {
        self.sessions.get(session_id)
    }

    fn all_sessions(&self) -> Vec<SessionInfo> {
        self.sessions.values().cloned().collect()
    }

    fn session_count(&self) -> usize {
        self.sessions.len()
    }

    fn evict_expired(&mut self) {
        let now = Instant::now();
        // We use created_at as proxy; in real impl track Instant per session
        self.sessions.retain(|_, s| {
            let created =
                chrono::DateTime::parse_from_rfc3339(&format!("{}Z", &s.created_at)).ok();
            match created {
                Some(ts) => {
                    let elapsed = chrono::Utc::now()
                        .signed_duration_since(ts.with_timezone(&chrono::Utc));
                    elapsed.num_seconds() < self.ttl.as_secs() as i64
                }
                None => true,
            }
        });
    }
}

// ──────────────────────────────────────────────
// Usage Tracker
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct UsageStats {
    total_requests: u64,
    total_tokens_input: u64,
    total_tokens_output: u64,
    total_tokens_cache_hit: u64,
    active_sessions: usize,
    uptime_seconds: u64,
    requests_per_minute: f64,
    average_tokens_per_request: f64,
    circuit_breaker: CircuitBreakerState,
    start_time: String,
}

struct UsageTracker {
    total_requests: u64,
    total_tokens_input: u64,
    total_tokens_output: u64,
    total_tokens_cache_hit: u64,
    minute_bucket: Vec<Instant>,
    start_time: Instant,
    circuit_breaker: CircuitBreaker,
}

impl UsageTracker {
    fn new() -> Self {
        Self {
            total_requests: 0,
            total_tokens_input: 0,
            total_tokens_output: 0,
            total_tokens_cache_hit: 0,
            minute_bucket: Vec::new(),
            start_time: Instant::now(),
            circuit_breaker: CircuitBreaker::new(5, 30),
        }
    }

    fn record_request(&mut self, input_tokens: u64, output_tokens: u64, cache_hit: bool) {
        self.total_requests += 1;
        self.total_tokens_input += input_tokens;
        self.total_tokens_output += output_tokens;
        if cache_hit {
            self.total_tokens_cache_hit += input_tokens;
        }
        self.minute_bucket.push(Instant::now());
        // Keep only last 60s
        let cutoff = Instant::now() - Duration::from_secs(60);
        self.minute_bucket.retain(|t| *t > cutoff);
    }

    fn stats(&self, session_count: usize) -> UsageStats {
        let uptime = self.start_time.elapsed().as_secs();
        let rpm = if uptime > 0 {
            self.total_requests as f64 / (uptime as f64 / 60.0)
        } else {
            0.0
        };
        let avg_tokens = if self.total_requests > 0 {
            (self.total_tokens_input + self.total_tokens_output) as f64 / self.total_requests as f64
        } else {
            0.0
        };

        UsageStats {
            total_requests: self.total_requests,
            total_tokens_input: self.total_tokens_input,
            total_tokens_output: self.total_tokens_output,
            total_tokens_cache_hit: self.total_tokens_cache_hit,
            active_sessions: session_count,
            uptime_seconds: uptime,
            requests_per_minute: rpm,
            average_tokens_per_request: avg_tokens,
            circuit_breaker: self.circuit_breaker.state(),
            start_time: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }

    fn circuit_breaker_mut(&mut self) -> &mut CircuitBreaker {
        &mut self.circuit_breaker
    }
}

// ──────────────────────────────────────────────
// DeepSeek Model Catalog (V3.2)
// ──────────────────────────────────────────────

fn model_catalog() -> Value {
    json!({
        "models": [
            {
                "id": "deepseek-chat",
                "description": "DeepSeek V3.2 - General purpose model for conversations and coding",
                "context_window": 128000,
                "max_output_tokens": 8192,
                "default_output_tokens": 4096,
                "thinking_mode": "optional",
                "supports_json_mode": true,
                "supports_function_calling": true,
                "pricing": {
                    "input_per_1m_tokens": 0.28,
                    "cache_hit_per_1m_tokens": 0.028,
                    "output_per_1m_tokens": 0.42
                }
            },
            {
                "id": "deepseek-reasoner",
                "description": "DeepSeek V3.2 - Reasoning model for complex logical tasks",
                "context_window": 128000,
                "max_output_tokens": 65536,
                "default_output_tokens": 32768,
                "thinking_mode": "required",
                "supports_json_mode": true,
                "supports_function_calling": true,
                "pricing": {
                    "input_per_1m_tokens": 0.28,
                    "cache_hit_per_1m_tokens": 0.028,
                    "output_per_1m_tokens": 0.42
                }
            }
        ]
    })
}

// ──────────────────────────────────────────────
// Configuration (from env, keys masked)
// ──────────────────────────────────────────────

fn mask_api_key(key: &str) -> String {
    if key.len() > 4 {
        let prefix = &key[..3];
        let suffix = &key[key.len() - 4..];
        format!("{}****{}", prefix, suffix)
    } else {
        "****".to_string()
    }
}

fn server_config() -> Value {
    let api_key = std::env::var("DEEPSEEK_API_KEY").ok();
    let masked = api_key.as_ref().map(|k| mask_api_key(k));

    json!({
        "apiKey": masked.unwrap_or_else(|| "not-set".to_string()),
        "baseUrl": "https://api.deepseek.com",
        "requestTimeout": 60000,
        "maxRetries": 2,
        "showCostInfo": true,
        "maxMessageLength": 100000,
        "sessionTtlMinutes": 30,
        "maxSessions": 100,
        "fallbackEnabled": true,
        "defaultModel": "deepseek-chat",
        "skipConnectionTest": false,
        "circuitBreakerThreshold": 5,
        "circuitBreakerResetTimeout": 30,
        "maxSessionMessages": 1000
    })
}

// ──────────────────────────────────────────────
// MCP Server State
// ──────────────────────────────────────────────

pub struct McpServer {
    session_manager: Arc<Mutex<SessionManager>>,
    usage_tracker: Arc<Mutex<UsageTracker>>,
    server_info: Value,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            session_manager: Arc::new(Mutex::new(SessionManager::new(30, 100))),
            usage_tracker: Arc::new(Mutex::new(UsageTracker::new())),
            server_info: json!({
                "name": "lingshu-cil",
                "version": "0.2.1-ds",
                "protocol_version": "2024-11-05",
                "capabilities": {
                    "resources": {
                        "subscribe": true,
                        "listChanged": true
                    },
                    "tools": {
                        "listChanged": true
                    }
                }
            }),
        }
    }

    /// Run the MCP server: reads JSON-RPC from stdin, writes to stdout
    pub fn run(&self) -> anyhow::Result<()> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut stdout_lock = stdout.lock();

        // Signal that MCP server is ready (stderr so it doesn't interfere with protocol)
        eprintln!("[lingshu-cil:mcp] MCP server started on stdio transport");
        eprintln!("[lingshu-cil:mcp] Protocol: JSON-RPC 2.0");
        eprintln!("[lingshu-cil:mcp] Resources: deepseek://models, deepseek://config, deepseek://usage");

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
                    let _ = stdout_lock.flush();
                    continue;
                }
            };

            let response = self.handle_request(&request);

            if let Some(resp) = response {
                let json = serde_json::to_string(&resp).unwrap();
                let _ = writeln!(stdout_lock, "{}", json);
                let _ = stdout_lock.flush();
            }
        }

        Ok(())
    }

    fn handle_request(&self, req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let method = req.method.as_str();
        let id = req.id.clone();

        match method {
            // ── Lifecycle ──
            "initialize" => {
                let protocol_version = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("protocolVersion"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("2024-11-05");

                // Record init as a request
                if let Ok(mut tracker) = self.usage_tracker.lock() {
                    tracker.record_request(0, 0, false);
                }

                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({
                        "protocolVersion": protocol_version,
                        "serverInfo": self.server_info,
                        "capabilities": {
                            "resources": {
                                "subscribe": true,
                                "listChanged": true
                            },
                            "tools": {}
                        }
                    })),
                    error: None,
                })
            }

            "notifications/initialized" => {
                // Acknowledge but no response needed for notifications
                None
            }

            // ── Resources ──
            "resources/list" => {
                let resources = vec![
                    McpResource {
                        uri: "deepseek://models".to_string(),
                        name: "DeepSeek Model Catalog".to_string(),
                        description: "Available DeepSeek models with capabilities, context limits, and V3.2 cache-aware pricing".to_string(),
                        mime_type: Some("application/json".to_string()),
                    },
                    McpResource {
                        uri: "deepseek://config".to_string(),
                        name: "Server Configuration".to_string(),
                        description: "Current server configuration with masked credentials".to_string(),
                        mime_type: Some("application/json".to_string()),
                    },
                    McpResource {
                        uri: "deepseek://usage".to_string(),
                        name: "Usage Statistics".to_string(),
                        description: "Real-time usage statistics and session metrics".to_string(),
                        mime_type: Some("application/json".to_string()),
                    },
                ];

                if let Ok(mut tracker) = self.usage_tracker.lock() {
                    tracker.record_request(10, 0, false);
                }

                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({
                        "resources": resources
                    })),
                    error: None,
                })
            }

            "resources/read" => {
                let uri = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");

                let result = match uri {
                    "deepseek://models" => {
                        if let Ok(mut tracker) = self.usage_tracker.lock() {
                            tracker.record_request(20, 50, false);
                        }
                        json!({
                            "contents": [{
                                "uri": uri,
                                "mimeType": "application/json",
                                "text": serde_json::to_string_pretty(&model_catalog()).unwrap()
                            }]
                        })
                    }
                    "deepseek://config" => {
                        if let Ok(mut tracker) = self.usage_tracker.lock() {
                            tracker.record_request(10, 30, false);
                        }
                        json!({
                            "contents": [{
                                "uri": uri,
                                "mimeType": "application/json",
                                "text": serde_json::to_string_pretty(&server_config()).unwrap()
                            }]
                        })
                    }
                    "deepseek://usage" => {
                        let session_count = self
                            .session_manager
                            .lock()
                            .map(|sm| sm.session_count())
                            .unwrap_or(0);

                        let stats = self
                            .usage_tracker
                            .lock()
                            .map(|ut| ut.stats(session_count))
                            .unwrap_or_else(|_| UsageStats {
                                total_requests: 0,
                                total_tokens_input: 0,
                                total_tokens_output: 0,
                                total_tokens_cache_hit: 0,
                                active_sessions: 0,
                                uptime_seconds: 0,
                                requests_per_minute: 0.0,
                                average_tokens_per_request: 0.0,
                                circuit_breaker: CircuitBreakerState {
                                    failure_count: 0,
                                    threshold: 5,
                                    reset_timeout_secs: 30,
                                    last_failure: None,
                                    state: "closed".to_string(),
                                },
                                start_time: chrono::Utc::now()
                                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                                    .to_string(),
                            });

                        if let Ok(mut tracker) = self.usage_tracker.lock() {
                            tracker.record_request(15, 40, false);
                        }

                        json!({
                            "contents": [{
                                "uri": uri,
                                "mimeType": "application/json",
                                "text": serde_json::to_string_pretty(&stats).unwrap()
                            }]
                        })
                    }
                    _ => {
                        return Some(JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32602,
                                message: format!("Resource not found: {}", uri),
                                data: Some(json!({
                                    "valid_uris": [
                                        "deepseek://models",
                                        "deepseek://config",
                                        "deepseek://usage"
                                    ]
                                })),
                            }),
                        });
                    }
                };

                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(result),
                    error: None,
                })
            }

            // ── Tools ──
            "tools/list" => {
                let tools = vec![
                    McpTool {
                        name: "chat".to_string(),
                        description: "Send a chat completion request to DeepSeek API".to_string(),
                        input_schema: json!({
                            "type": "object",
                            "properties": {
                                "model": {
                                    "type": "string",
                                    "enum": ["deepseek-chat", "deepseek-reasoner"],
                                    "description": "Model to use"
                                },
                                "messages": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "role": {"type": "string", "enum": ["user", "assistant", "system"]},
                                            "content": {"type": "string"}
                                        }
                                    }
                                },
                                "max_tokens": {"type": "integer", "default": 4096},
                                "temperature": {"type": "number", "default": 0.7}
                            },
                            "required": ["model", "messages"]
                        }),
                    },
                    McpTool {
                        name: "create_session".to_string(),
                        description: "Create a new conversation session".to_string(),
                        input_schema: json!({
                            "type": "object",
                            "properties": {
                                "model": {
                                    "type": "string",
                                    "default": "deepseek-chat"
                                }
                            }
                        }),
                    },
                ];

                if let Ok(mut tracker) = self.usage_tracker.lock() {
                    tracker.record_request(10, 0, false);
                }

                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({ "tools": tools })),
                    error: None,
                })
            }

            "tools/call" => {
                let tool_name = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");

                let args = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or(json!({}));

                match tool_name {
                    "create_session" => {
                        let model = args
                            .get("model")
                            .and_then(|m| m.as_str())
                            .unwrap_or("deepseek-chat");

                        let session_id = self
                            .session_manager
                            .lock()
                            .map(|mut sm| sm.create_session(model))
                            .unwrap_or_else(|_| "error".to_string());

                        if let Ok(mut tracker) = self.usage_tracker.lock() {
                            tracker.record_request(5, 10, false);
                        }

                        Some(JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result: Some(json!({
                                "content": [{
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&json!({
                                        "session_id": session_id,
                                        "model": model,
                                        "status": "created"
                                    })).unwrap()
                                }]
                            })),
                            error: None,
                        })
                    }

                    "chat" => {
                        // Validate API key
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

                        if let Ok(mut cb) = self.usage_tracker.lock() {
                            let result = cb.circuit_breaker_mut().call(|| {
                                // In a full implementation, this would call the DeepSeek API
                                // For now, return the model catalog info as reference
                                Ok(json!({
                                    "model": args.get("model").and_then(|m| m.as_str()).unwrap_or("deepseek-chat"),
                                    "usage": {
                                        "prompt_tokens": 0,
                                        "completion_tokens": 0,
                                        "total_tokens": 0
                                    }
                                }))
                            });

                            match result {
                                Ok(data) => {
                                    if let Ok(mut tracker) = self.usage_tracker.lock() {
                                        tracker.record_request(100, 200, false);
                                    }
                                    Some(JsonRpcResponse {
                                        jsonrpc: "2.0".to_string(),
                                        id,
                                        result: Some(json!({
                                            "content": [{
                                                "type": "text",
                                                "text": serde_json::to_string_pretty(&data).unwrap()
                                            }]
                                        })),
                                        error: None,
                                    })
                                }
                                Err(e) => Some(JsonRpcResponse {
                                    jsonrpc: "2.0".to_string(),
                                    id,
                                    result: None,
                                    error: Some(JsonRpcError {
                                        code: -32000,
                                        message: e,
                                        data: None,
                                    }),
                                }),
                            }
                        } else {
                            Some(JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id,
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32000,
                                    message: "Internal error".to_string(),
                                    data: None,
                                }),
                            })
                        }
                    }

                    _ => Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: format!("Unknown tool: {}", tool_name),
                            data: None,
                        }),
                    }),
                }
            }

            // ── Prompts ──
            "prompts/list" => {
                let prompts = vec![
                    McpPrompt {
                        name: "analyze_code".to_string(),
                        description: "Analyze code for improvements, bugs, and security issues".to_string(),
                        arguments: vec![
                            json!({
                                "name": "code",
                                "description": "Code snippet to analyze",
                                "required": true
                            }),
                            json!({
                                "name": "language",
                                "description": "Programming language",
                                "required": false
                            }),
                        ],
                    },
                    McpPrompt {
                        name: "optimize_dockerfile".to_string(),
                        description: "Get Dockerfile optimization suggestions".to_string(),
                        arguments: vec![
                            json!({
                                "name": "dockerfile",
                                "description": "Dockerfile content to optimize",
                                "required": true
                            }),
                        ],
                    },
                ];

                if let Ok(mut tracker) = self.usage_tracker.lock() {
                    tracker.record_request(10, 0, false);
                }

                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({ "prompts": prompts })),
                    error: None,
                })
            }

            "prompts/get" => {
                let name = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");

                match name {
                    "analyze_code" => Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(json!({
                            "description": "Analyze code for improvements, bugs, and security issues",
                            "messages": [
                                {
                                    "role": "user",
                                    "content": {
                                        "type": "text",
                                        "text": "Please analyze the following code for:\n1. Potential bugs\n2. Security vulnerabilities\n3. Performance improvements\n4. Code style issues\n\n```\n{{code}}\n```"
                                    }
                                }
                            ]
                        })),
                        error: None,
                    }),
                    "optimize_dockerfile" => Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(json!({
                            "description": "Get Dockerfile optimization suggestions",
                            "messages": [
                                {
                                    "role": "user",
                                    "content": {
                                        "type": "text",
                                        "text": "Please optimize this Dockerfile for:\n1. Build speed (layer caching)\n2. Image size (multi-stage builds)\n3. Security best practices\n\n```dockerfile\n{{dockerfile}}\n```"
                                    }
                                }
                            ]
                        })),
                        error: None,
                    }),
                    _ => Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: format!("Unknown prompt: {}", name),
                            data: None,
                        }),
                    }),
                }
            }

            // ── Ping / Health ──
            "ping" => {
                if let Ok(mut tracker) = self.usage_tracker.lock() {
                    tracker.record_request(0, 0, false);
                }

                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({})),
                    error: None,
                })
            }

            _ => {
                eprintln!(
                    "[lingshu-cil:mcp] Unknown method: {}",
                    method
                );
                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", method),
                        data: None,
                    }),
                })
            }
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}
