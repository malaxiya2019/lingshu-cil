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
    /// DeepSeek V3.2 context window (128K)
    pub context_window: u32,
    /// Whether thinking mode is required/optional/unsupported
    pub thinking_mode: String,
    /// V3.2 cache-aware pricing
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
            ModelConfig {
                name: "deepseek-chat".into(),
                display_name: "DeepSeek Chat".into(),
                provider: "deepseek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.7,
                context_window: 128000,
                thinking_mode: "optional".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 0.28,
                    cache_hit_per_1m_tokens: 0.028,
                    output_per_1m_tokens: 0.42,
                },
            },
            ModelConfig {
                name: "deepseek-reasoner".into(),
                display_name: "DeepSeek Reasoner".into(),
                provider: "deepseek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
                max_tokens: 65536,
                temperature: 0.2,
                context_window: 128000,
                thinking_mode: "required".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 0.28,
                    cache_hit_per_1m_tokens: 0.028,
                    output_per_1m_tokens: 0.42,
                },
            },
            ModelConfig {
                name: "deepseek-coder".into(),
                display_name: "DeepSeek Coder".into(),
                provider: "deepseek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.2,
                context_window: 128000,
                thinking_mode: "optional".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 0.28,
                    cache_hit_per_1m_tokens: 0.028,
                    output_per_1m_tokens: 0.42,
                },
            },
            ModelConfig {
                name: "gpt-4o".into(),
                display_name: "GPT-4o".into(),
                provider: "openai".into(),
                base_url: "https://api.openai.com/v1".into(),
                api_key: std::env::var("OPENAI_API_KEY").ok(),
                max_tokens: 16384,
                temperature: 0.7,
                context_window: 128000,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 2.50,
                    cache_hit_per_1m_tokens: 1.25,
                    output_per_1m_tokens: 10.00,
                },
            },
            ModelConfig {
                name: "qwen-plus".into(),
                display_name: "Qwen Plus".into(),
                provider: "qwen".into(),
                base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
                api_key: std::env::var("QWEN_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.7,
                context_window: 131072,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 0.80,
                    cache_hit_per_1m_tokens: 0.00,
                    output_per_1m_tokens: 2.00,
                },
            },
            ModelConfig {
                name: "qwen-turbo".into(),
                display_name: "Qwen Turbo".into(),
                provider: "qwen".into(),
                base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
                api_key: std::env::var("QWEN_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.7,
                context_window: 131072,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 0.30,
                    cache_hit_per_1m_tokens: 0.00,
                    output_per_1m_tokens: 0.60,
                },
            },
            ModelConfig {
                name: "qwen-max".into(),
                display_name: "Qwen Max".into(),
                provider: "qwen".into(),
                base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
                api_key: std::env::var("QWEN_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.7,
                context_window: 32768,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 2.00,
                    cache_hit_per_1m_tokens: 0.00,
                    output_per_1m_tokens: 6.00,
                },
            },
            ModelConfig {
                name: "claude-3-sonnet".into(),
                display_name: "Claude 3 Sonnet".into(),
                provider: "anthropic".into(),
                base_url: "https://api.anthropic.com/v1".into(),
                api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.5,
                context_window: 200000,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 3.00,
                    cache_hit_per_1m_tokens: 0.30,
                    output_per_1m_tokens: 15.00,
                },
            },
            ModelConfig {
                name: "gemini-2.0-flash".into(),
                display_name: "Gemini 2.0 Flash".into(),
                provider: "gemini".into(),
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                api_key: std::env::var("GEMINI_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.7,
                context_window: 1048576,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 0.10,
                    cache_hit_per_1m_tokens: 0.025,
                    output_per_1m_tokens: 0.40,
                },
            },
            ModelConfig {
                name: "gemini-2.5-pro".into(),
                display_name: "Gemini 2.5 Pro".into(),
                provider: "gemini".into(),
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                api_key: std::env::var("GEMINI_API_KEY").ok(),
                max_tokens: 65536,
                temperature: 0.7,
                context_window: 1048576,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 1.25,
                    cache_hit_per_1m_tokens: 0.10,
                    output_per_1m_tokens: 5.00,
                },
            },
            ModelConfig {
                name: "moonshot-v1".into(),
                display_name: "Moonshot V1".into(),
                provider: "moonshot".into(),
                base_url: "https://api.moonshot.cn/v1".into(),
                api_key: std::env::var("MOONSHOT_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.7,
                context_window: 131072,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 1.00,
                    cache_hit_per_1m_tokens: 0.00,
                    output_per_1m_tokens: 2.00,
                },
            },
            ModelConfig {
                name: "moonshot-v1-32k".into(),
                display_name: "Moonshot V1 32K".into(),
                provider: "moonshot".into(),
                base_url: "https://api.moonshot.cn/v1".into(),
                api_key: std::env::var("MOONSHOT_API_KEY").ok(),
                max_tokens: 8192,
                temperature: 0.7,
                context_window: 32768,
                thinking_mode: "unsupported".into(),
                pricing: ModelPricing {
                    input_per_1m_tokens: 2.00,
                    cache_hit_per_1m_tokens: 0.00,
                    output_per_1m_tokens: 4.00,
                },
            },
        ]
    }
}

impl fmt::Display for ModelConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}) · ctx: {} · thinking: {} · ${:.2}/1M in",
            self.display_name,
            self.name,
            self.context_window,
            self.thinking_mode,
            self.pricing.input_per_1m_tokens,
        )
    }
}

/// Permission modes for the CIL agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionMode {
    FullContext,
    SuggestOnly,
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

/// Stream response from LLM API
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
