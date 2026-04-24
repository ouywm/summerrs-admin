//! Adapter 协议/风味描述与当前 dispatcher 使用的 [`AdapterKind`]。
//!
//! `AdapterKind` 仍然保留，作为当前 [`super::AdapterDispatcher`] 的静态分派键。
//! 但数据库路由不应再直接绑定它；运行时路由应优先使用
//! [`AdapterDescriptor`] = `ProtocolKind + FlavorKind`。

use serde::{Deserialize, Serialize};

/// 当前 dispatcher 使用的上游适配器键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdapterKind {
    // ─── 1-4: OpenAI 家族 ───
    /// OpenAI 官方 (`api.openai.com`) 的 `/v1/chat/completions`。
    OpenAI,
    /// OpenAI `/v1/responses` API（GPT-5 / o1 等 reasoning 模型）。
    OpenAIResp,
    /// OpenAI 兼容第三方（兜底变体，厂商无 native 适配时用）。
    OpenAICompat,
    /// Azure OpenAI Service。
    Azure,

    // ─── 5-8: Native 协议 ───
    /// Claude `/v1/messages`。
    Claude,
    /// Google Gemini `generateContent`。
    Gemini,
    /// Cohere native。
    Cohere,
    /// Ollama native (`localhost:11434`)。
    Ollama,

    // ─── 9-21: OpenAI-compat 变种（有 native 细节差异）───
    /// Ollama Cloud（`ollama.com`，Bearer 鉴权）。
    OllamaCloud,
    /// Groq。
    Groq,
    /// DeepSeek。
    DeepSeek,
    /// xAI (Grok)。
    Xai,
    /// Fireworks AI。
    Fireworks,
    /// Together AI。
    Together,
    /// Nebius AI Studio。
    Nebius,
    /// Mimo。
    Mimo,
    /// Z.AI (原 ChatGLM / 智谱)。
    Zai,
    /// BigModel（智谱 Open Platform）。
    BigModel,
    /// 阿里云 Dashscope / 百炼。
    Aliyun,
    /// Google Vertex AI（支持 Gemini + Claude）。
    Vertex,
    /// GitHub Models（OpenAI/Claude/Google 聚合）。
    GithubCopilot,
}

impl AdapterKind {
    /// 所有 21 个变体的数组（顺序即编码值）。
    pub const ALL: [AdapterKind; 21] = [
        Self::OpenAI,
        Self::OpenAIResp,
        Self::OpenAICompat,
        Self::Azure,
        Self::Claude,
        Self::Gemini,
        Self::Cohere,
        Self::Ollama,
        Self::OllamaCloud,
        Self::Groq,
        Self::DeepSeek,
        Self::Xai,
        Self::Fireworks,
        Self::Together,
        Self::Nebius,
        Self::Mimo,
        Self::Zai,
        Self::BigModel,
        Self::Aliyun,
        Self::Vertex,
        Self::GithubCopilot,
    ];

    /// 稳定英文名（日志/API 响应用）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::OpenAIResp => "OpenAIResp",
            Self::OpenAICompat => "OpenAICompat",
            Self::Azure => "Azure",
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
            Self::Cohere => "Cohere",
            Self::Ollama => "Ollama",
            Self::OllamaCloud => "OllamaCloud",
            Self::Groq => "Groq",
            Self::DeepSeek => "DeepSeek",
            Self::Xai => "Xai",
            Self::Fireworks => "Fireworks",
            Self::Together => "Together",
            Self::Nebius => "Nebius",
            Self::Mimo => "Mimo",
            Self::Zai => "Zai",
            Self::BigModel => "BigModel",
            Self::Aliyun => "Aliyun",
            Self::Vertex => "Vertex",
            Self::GithubCopilot => "GithubCopilot",
        }
    }

    /// 小写下划线形式（DB / 配置用）。
    pub const fn as_lower_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::OpenAIResp => "openai_resp",
            Self::OpenAICompat => "openai_compat",
            Self::Azure => "azure",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Cohere => "cohere",
            Self::Ollama => "ollama",
            Self::OllamaCloud => "ollama_cloud",
            Self::Groq => "groq",
            Self::DeepSeek => "deepseek",
            Self::Xai => "xai",
            Self::Fireworks => "fireworks",
            Self::Together => "together",
            Self::Nebius => "nebius",
            Self::Mimo => "mimo",
            Self::Zai => "zai",
            Self::BigModel => "bigmodel",
            Self::Aliyun => "aliyun",
            Self::Vertex => "vertex",
            Self::GithubCopilot => "github_copilot",
        }
    }

    /// 协议默认的 API Key 环境变量名（dev 环境 fallback）。
    pub const fn default_api_key_env_name(&self) -> Option<&'static str> {
        match self {
            Self::OpenAI | Self::OpenAIResp => Some("OPENAI_API_KEY"),
            Self::Azure => Some("AZURE_OPENAI_API_KEY"),
            Self::Claude => Some("ANTHROPIC_API_KEY"),
            Self::Gemini => Some("GEMINI_API_KEY"),
            Self::Cohere => Some("COHERE_API_KEY"),
            Self::Groq => Some("GROQ_API_KEY"),
            Self::DeepSeek => Some("DEEPSEEK_API_KEY"),
            Self::Xai => Some("XAI_API_KEY"),
            Self::Fireworks => Some("FIREWORKS_API_KEY"),
            Self::Together => Some("TOGETHER_API_KEY"),
            Self::Nebius => Some("NEBIUS_API_KEY"),
            Self::Mimo => Some("MIMO_API_KEY"),
            Self::Zai => Some("ZAI_API_KEY"),
            Self::BigModel => Some("BIGMODEL_API_KEY"),
            Self::Aliyun => Some("DASHSCOPE_API_KEY"),
            Self::OllamaCloud => Some("OLLAMA_API_KEY"),
            Self::GithubCopilot => Some("GITHUB_TOKEN"),
            Self::Vertex => Some("GOOGLE_APPLICATION_CREDENTIALS"),
            // OpenAICompat 和 Ollama（本地）没有事实默认 env
            Self::OpenAICompat | Self::Ollama => None,
        }
    }

    /// 从小写字符串解析（配置 / 兼容性）。
    pub fn from_lower_str(name: &str) -> Option<Self> {
        Some(match name {
            "openai" => Self::OpenAI,
            "openai_resp" => Self::OpenAIResp,
            "openai_compat" => Self::OpenAICompat,
            "azure" => Self::Azure,
            "claude" => Self::Claude,
            "gemini" => Self::Gemini,
            "cohere" => Self::Cohere,
            "ollama" => Self::Ollama,
            "ollama_cloud" => Self::OllamaCloud,
            "groq" => Self::Groq,
            "deepseek" => Self::DeepSeek,
            "xai" => Self::Xai,
            "fireworks" => Self::Fireworks,
            "together" => Self::Together,
            "nebius" => Self::Nebius,
            "mimo" => Self::Mimo,
            "zai" => Self::Zai,
            "bigmodel" => Self::BigModel,
            "aliyun" => Self::Aliyun,
            "vertex" => Self::Vertex,
            "github_copilot" => Self::GithubCopilot,
            _ => return None,
        })
    }
}

impl std::fmt::Display for AdapterKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 协议维度。
///
/// 这里只描述 wire/API 规范，不承载厂商/部署信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProtocolKind {
    OpenAI,
    OpenAIResponses,
    Claude,
    Gemini,
    Cohere,
    Ollama,
}

impl ProtocolKind {
    pub const fn as_lower_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::OpenAIResponses => "openai_responses",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Cohere => "cohere",
            Self::Ollama => "ollama",
        }
    }

    pub fn from_lower_str(name: &str) -> Option<Self> {
        Some(match name {
            "openai" => Self::OpenAI,
            "openai_responses" => Self::OpenAIResponses,
            "claude" => Self::Claude,
            "gemini" => Self::Gemini,
            "cohere" => Self::Cohere,
            "ollama" => Self::Ollama,
            _ => return None,
        })
    }
}

/// 部署/兼容层维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlavorKind {
    Native,
    OpenAICompat,
    Azure,
    OllamaCloud,
    Groq,
    DeepSeek,
    Xai,
    Fireworks,
    Together,
    Nebius,
    Mimo,
    Zai,
    BigModel,
    Aliyun,
    Vertex,
    GithubCopilot,
}

impl FlavorKind {
    pub const fn as_lower_str(&self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::OpenAICompat => "openai_compat",
            Self::Azure => "azure",
            Self::OllamaCloud => "ollama_cloud",
            Self::Groq => "groq",
            Self::DeepSeek => "deepseek",
            Self::Xai => "xai",
            Self::Fireworks => "fireworks",
            Self::Together => "together",
            Self::Nebius => "nebius",
            Self::Mimo => "mimo",
            Self::Zai => "zai",
            Self::BigModel => "bigmodel",
            Self::Aliyun => "aliyun",
            Self::Vertex => "vertex",
            Self::GithubCopilot => "github_copilot",
        }
    }

    pub fn from_lower_str(name: &str) -> Option<Self> {
        Some(match name {
            "native" => Self::Native,
            "openai_compat" => Self::OpenAICompat,
            "azure" => Self::Azure,
            "ollama_cloud" => Self::OllamaCloud,
            "groq" => Self::Groq,
            "deepseek" => Self::DeepSeek,
            "xai" => Self::Xai,
            "fireworks" => Self::Fireworks,
            "together" => Self::Together,
            "nebius" => Self::Nebius,
            "mimo" => Self::Mimo,
            "zai" => Self::Zai,
            "bigmodel" => Self::BigModel,
            "aliyun" => Self::Aliyun,
            "vertex" => Self::Vertex,
            "github_copilot" => Self::GithubCopilot,
            _ => return None,
        })
    }
}

/// 运行时适配器描述：协议 + 风味。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterDescriptor {
    pub protocol: ProtocolKind,
    pub flavor: FlavorKind,
}

impl AdapterDescriptor {
    pub const fn new(protocol: ProtocolKind, flavor: FlavorKind) -> Self {
        Self { protocol, flavor }
    }

    /// 过渡桥：把 descriptor 映射回当前 dispatcher 使用的 `AdapterKind`。
    pub const fn try_adapter_kind(&self) -> Option<AdapterKind> {
        Some(match (self.protocol, self.flavor) {
            (ProtocolKind::OpenAI, FlavorKind::Native) => AdapterKind::OpenAI,
            (ProtocolKind::OpenAIResponses, FlavorKind::Native) => AdapterKind::OpenAIResp,
            (ProtocolKind::OpenAI, FlavorKind::OpenAICompat) => AdapterKind::OpenAICompat,
            (ProtocolKind::OpenAI, FlavorKind::Azure) => AdapterKind::Azure,
            (ProtocolKind::Claude, FlavorKind::Native) => AdapterKind::Claude,
            (ProtocolKind::Gemini, FlavorKind::Native) => AdapterKind::Gemini,
            (ProtocolKind::Cohere, FlavorKind::Native) => AdapterKind::Cohere,
            (ProtocolKind::Ollama, FlavorKind::Native) => AdapterKind::Ollama,
            (ProtocolKind::OpenAI, FlavorKind::OllamaCloud) => AdapterKind::OllamaCloud,
            (ProtocolKind::OpenAI, FlavorKind::Groq) => AdapterKind::Groq,
            (ProtocolKind::OpenAI, FlavorKind::DeepSeek) => AdapterKind::DeepSeek,
            (ProtocolKind::OpenAI, FlavorKind::Xai) => AdapterKind::Xai,
            (ProtocolKind::OpenAI, FlavorKind::Fireworks) => AdapterKind::Fireworks,
            (ProtocolKind::OpenAI, FlavorKind::Together) => AdapterKind::Together,
            (ProtocolKind::OpenAI, FlavorKind::Nebius) => AdapterKind::Nebius,
            (ProtocolKind::OpenAI, FlavorKind::Mimo) => AdapterKind::Mimo,
            (ProtocolKind::OpenAI, FlavorKind::Zai) => AdapterKind::Zai,
            (ProtocolKind::OpenAI, FlavorKind::BigModel) => AdapterKind::BigModel,
            (ProtocolKind::OpenAI, FlavorKind::Aliyun) => AdapterKind::Aliyun,
            (ProtocolKind::Gemini, FlavorKind::Vertex)
            | (ProtocolKind::Claude, FlavorKind::Vertex) => AdapterKind::Vertex,
            (ProtocolKind::OpenAI, FlavorKind::GithubCopilot)
            | (ProtocolKind::Claude, FlavorKind::GithubCopilot)
            | (ProtocolKind::Gemini, FlavorKind::GithubCopilot) => AdapterKind::GithubCopilot,
            _ => return None,
        })
    }

    pub fn adapter_kind(&self) -> AdapterKind {
        self.try_adapter_kind()
            .expect("adapter descriptor should map to an existing adapter kind")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_roundtrip_via_lower_str() {
        for kind in AdapterKind::ALL {
            let s = kind.as_lower_str();
            let back = AdapterKind::from_lower_str(s).unwrap();
            assert_eq!(kind, back, "lower_str roundtrip failed for {kind:?} ({s})");
        }
    }

    #[test]
    fn descriptor_maps_to_existing_adapter_kind() {
        let descriptor = AdapterDescriptor::new(ProtocolKind::OpenAIResponses, FlavorKind::Native);
        assert_eq!(descriptor.adapter_kind(), AdapterKind::OpenAIResp);

        let compat = AdapterDescriptor::new(ProtocolKind::OpenAI, FlavorKind::OpenAICompat);
        assert_eq!(compat.adapter_kind(), AdapterKind::OpenAICompat);
    }

    #[test]
    fn protocol_and_flavor_parse_from_lower_str() {
        assert_eq!(
            ProtocolKind::from_lower_str("openai_responses"),
            Some(ProtocolKind::OpenAIResponses)
        );
        assert_eq!(
            FlavorKind::from_lower_str("openai_compat"),
            Some(FlavorKind::OpenAICompat)
        );
        assert_eq!(ProtocolKind::from_lower_str("bogus"), None);
        assert_eq!(FlavorKind::from_lower_str("bogus"), None);
    }

    #[test]
    fn default_env_names_sanity() {
        assert_eq!(
            AdapterKind::OpenAI.default_api_key_env_name(),
            Some("OPENAI_API_KEY")
        );
        assert!(
            AdapterKind::OpenAICompat
                .default_api_key_env_name()
                .is_none()
        );
    }
}
