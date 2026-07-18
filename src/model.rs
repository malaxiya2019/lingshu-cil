use serde::{Deserialize, Serialize};
use std::fmt;

// ── Model Configuration ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub display_name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub context_window: u32,
    pub thinking_mode: String,
    pub pricing: ModelPricing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_1m_tokens: f64,
    pub cache_hit_per_1m_tokens: f64,
    pub output_per_1m_tokens: f64,
}

impl ModelConfig {
    pub fn builtins() -> Vec<Self> {
        vec![
            Self {
                name: "deepseek-chat".into(),
                display_name: "DeepSeek Chat".into(),
                provider: "deepseek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
                max_tokens: 8192, temperature: 0.7, context_window: 128000,
                thinking_mode: "optional".into(),
                pricing: ModelPricing { input_per_1m_tokens: 0.28, cache_hit_per_1m_tokens: 0.028, output_per_1m_tokens: 0.42 },
            },
            Self {
                name: "deepseek-coder".into(), display_name: "DeepSeek Coder".into(),
                provider: "deepseek".into(), base_url: "https://api.deepseek.com/v1".into(),
                api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
                max_tokens: 8192, temperature: 0.2, context_window: 128000,
                thinking_mode: "optional".into(),
                pricing: ModelPricing { input_per_1m_tokens: 0.28, cache_hit_per_1m_tokens: 0.028, output_per_1m_tokens: 0.42 },
            },
            Self {
                name: "gpt-4o".into(), display_name: "GPT-4o".into(),
                provider: "openai".into(), base_url: "https://api.openai.com/v1".into(),
                api_key: std::env::var("OPENAI_API_KEY").ok(),
                max_tokens: 16384, temperature: 0.7, context_window: 128000,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing { input_per_1m_tokens: 2.50, cache_hit_per_1m_tokens: 1.25, output_per_1m_tokens: 10.00 },
            },
            Self {
                name: "qwen-plus".into(), display_name: "Qwen Plus".into(),
                provider: "qwen".into(), base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
                api_key: std::env::var("QWEN_API_KEY").ok(),
                max_tokens: 8192, temperature: 0.7, context_window: 131072,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing { input_per_1m_tokens: 0.80, cache_hit_per_1m_tokens: 0.00, output_per_1m_tokens: 2.00 },
            },
        ]
    }
}

impl fmt::Display for ModelConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.display_name, self.name)
    }
}

// ── Tool Definitions ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

// ── LLM Message ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl LlmMessage {
    pub fn system(content: &str) -> Self {
        Self { role: "system".into(), content: content.into(), tool_calls: None, tool_call_id: None }
    }
    pub fn user(content: &str) -> Self {
        Self { role: "user".into(), content: content.into(), tool_calls: None, tool_call_id: None }
    }
    pub fn assistant(content: &str, tool_calls: Option<Vec<ToolCall>>) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_calls, tool_call_id: None }
    }
    pub fn tool(content: &str, call_id: &str) -> Self {
        Self { role: "tool".into(), content: content.into(), tool_calls: None, tool_call_id: Some(call_id.into()) }
    }
}

// ── Task ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
    Failed(String),
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::InProgress => write!(f, "in-progress"),
            TaskStatus::Done => write!(f, "done"),
            TaskStatus::Failed(e) => write!(f, "failed: {}", e),
        }
    }
}

// ── SSE Stream ──

#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
pub struct StreamChoice {
    pub delta: Delta,
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct DeltaToolCall {
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
#[allow(dead_code)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub function: Option<DeltaFunction>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct DeltaFunction {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

// ── Permission Mode ──

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PermissionMode {
    Strict,
    Normal,
    Yolo,
}

impl PermissionMode {
    pub fn as_str(&self) -> &'static str {
        match self { PermissionMode::Strict => "strict", PermissionMode::Normal => "normal", PermissionMode::Yolo => "yolo" }
    }
}

impl fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
