use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub display_name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl ModelConfig {
    pub fn builtins() -> Vec<Self> {
        vec![
            ModelConfig {
                name: "deepseek-chat".into(),
                display_name: "DeepSeek Chat".into(),
                provider: "deepseek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.7,
            },
            ModelConfig {
                name: "deepseek-coder".into(),
                display_name: "DeepSeek Coder".into(),
                provider: "deepseek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.2,
            },
            ModelConfig {
                name: "gpt-4o".into(),
                display_name: "GPT-4o".into(),
                provider: "openai".into(),
                base_url: "https://api.openai.com/v1".into(),
                api_key: std::env::var("OPENAI_API_KEY").ok(),
                max_tokens: 16384,
                temperature: 0.7,
            },
            ModelConfig {
                name: "claude-3-sonnet".into(),
                display_name: "Claude 3 Sonnet".into(),
                provider: "anthropic".into(),
                base_url: "https://api.anthropic.com/v1".into(),
                api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.5,
            },
        ]
    }
}

impl fmt::Display for ModelConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.display_name, self.name)
    }
}

/// Permission modes for the CIL agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionMode {
    /// Read-only context awareness
    FullContext,
    /// Allow read + suggest commands
    SuggestOnly,
    /// Full autonomy - allow executing commands
    Yolo,
}

impl PermissionMode {
    pub fn next(&self) -> Self {
        match self {
            PermissionMode::FullContext => PermissionMode::SuggestOnly,
            PermissionMode::SuggestOnly => PermissionMode::Yolo,
            PermissionMode::Yolo => PermissionMode::FullContext,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionMode::FullContext => "Full Context",
            PermissionMode::SuggestOnly => "Suggest Only",
            PermissionMode::Yolo => "YOLO Mode",
        }
    }
}

impl fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

impl Message {
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Local::now()
                .format("%H:%M:%S")
                .to_string(),
        }
    }

    pub fn is_user(&self) -> bool {
        self.role == "user"
    }

    pub fn is_assistant(&self) -> bool {
        self.role == "assistant"
    }

    pub fn is_system(&self) -> bool {
        self.role == "system"
    }
}

/// Response from the LLM API (streaming chunk)
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
    pub role: Option<String>,
}
