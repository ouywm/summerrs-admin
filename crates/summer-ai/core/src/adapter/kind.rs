//! [`AdapterKind`] —— 上游协议家族枚举。
//!
//! **21 连续编码**：
//!
//! - 1-4  OpenAI 家族（OpenAI / OpenAIResp / OpenAICompat / Azure）
//! - 5-8  Native 协议（Anthropic / Gemini / Cohere / Ollama）
//! - 9-21 OpenAI-compat 变种（OllamaCloud / Groq / DeepSeek / Xai / ...）
//!
//! Relay 通过 `ai.channel.channel_type: i16` 字段选 AdapterKind；
//! 再由 [`super::AdapterDispatcher`] 静态 match 分派到具体 Adapter 实现。

use serde::{Deserialize, Serialize};

/// 上游协议家族。
///
/// **编码值一旦上生产禁止变更**（DB 已存的 `channel_type` 列不可改语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdapterKind {
    // ─── 1-4: OpenAI 家族 ───
    /// OpenAI 官方 (`api.openai.com`) 的 `/v1/chat/completions`。
    OpenAI = 1,
    /// OpenAI `/v1/responses` API（GPT-5 / o1 等 reasoning 模型）。
    OpenAIResp = 2,
    /// OpenAI 兼容第三方（兜底变体，厂商无 native 适配时用）。
    OpenAICompat = 3,
    /// Azure OpenAI Service。
    Azure = 4,

    // ─── 5-8: Native 协议 ───
    /// Anthropic `/v1/messages`。
    Anthropic = 5,
    /// Google Gemini `generateContent`。
    Gemini = 6,
    /// Cohere native。
    Cohere = 7,
    /// Ollama native (`localhost:11434`)。
    Ollama = 8,

    // ─── 9-21: OpenAI-compat 变种（有 native 细节差异）───
    /// Ollama Cloud（`ollama.com`，Bearer 鉴权）。
    OllamaCloud = 9,
    /// Groq。
    Groq = 10,
    /// DeepSeek。
    DeepSeek = 11,
    /// xAI (Grok)。
    Xai = 12,
    /// Fireworks AI。
    Fireworks = 13,
    /// Together AI。
    Together = 14,
    /// Nebius AI Studio。
    Nebius = 15,
    /// Mimo。
    Mimo = 16,
    /// Z.AI (原 ChatGLM / 智谱)。
    Zai = 17,
    /// BigModel（智谱 Open Platform）。
    BigModel = 18,
    /// 阿里云 Dashscope / 百炼。
    Aliyun = 19,
    /// Google Vertex AI（支持 Gemini + Anthropic）。
    Vertex = 20,
    /// GitHub Models（OpenAI/Anthropic/Google 聚合）。
    GithubCopilot = 21,
}

impl AdapterKind {
    /// 所有 21 个变体的数组（顺序即编码值）。
    pub const ALL: [AdapterKind; 21] = [
        Self::OpenAI,
        Self::OpenAIResp,
        Self::OpenAICompat,
        Self::Azure,
        Self::Anthropic,
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
            Self::Anthropic => "Anthropic",
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
            Self::Anthropic => "anthropic",
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
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
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
            "anthropic" => Self::Anthropic,
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

// ─── i16 ↔ AdapterKind（绑定 ai.channel.channel_type 列）───

impl TryFrom<i16> for AdapterKind {
    type Error = InvalidAdapterKind;
    fn try_from(value: i16) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::OpenAI,
            2 => Self::OpenAIResp,
            3 => Self::OpenAICompat,
            4 => Self::Azure,
            5 => Self::Anthropic,
            6 => Self::Gemini,
            7 => Self::Cohere,
            8 => Self::Ollama,
            9 => Self::OllamaCloud,
            10 => Self::Groq,
            11 => Self::DeepSeek,
            12 => Self::Xai,
            13 => Self::Fireworks,
            14 => Self::Together,
            15 => Self::Nebius,
            16 => Self::Mimo,
            17 => Self::Zai,
            18 => Self::BigModel,
            19 => Self::Aliyun,
            20 => Self::Vertex,
            21 => Self::GithubCopilot,
            other => return Err(InvalidAdapterKind(other)),
        })
    }
}

impl From<AdapterKind> for i16 {
    fn from(kind: AdapterKind) -> Self {
        kind as i16
    }
}

/// `TryFrom<i16>` 失败时的错误（`channel_type` 值在 1-21 范围外）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAdapterKind(pub i16);

impl std::fmt::Display for InvalidAdapterKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid channel_type i16: {}", self.0)
    }
}

impl std::error::Error for InvalidAdapterKind {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_roundtrip_via_i16() {
        for kind in AdapterKind::ALL {
            let n: i16 = kind.into();
            let back = AdapterKind::try_from(n).unwrap();
            assert_eq!(kind, back, "roundtrip failed for {kind:?} (i16={n})");
        }
    }

    #[test]
    fn all_variants_roundtrip_via_lower_str() {
        for kind in AdapterKind::ALL {
            let s = kind.as_lower_str();
            let back = AdapterKind::from_lower_str(s).unwrap();
            assert_eq!(kind, back, "lower_str roundtrip failed for {kind:?} ({s})");
        }
    }

    #[test]
    fn discriminants_are_1_to_21_continuous() {
        for (i, kind) in AdapterKind::ALL.iter().enumerate() {
            let expected = (i + 1) as i16;
            let actual: i16 = (*kind).into();
            assert_eq!(expected, actual, "变体 {kind:?} 编码应为 {expected}");
        }
    }

    #[test]
    fn try_from_i16_rejects_out_of_range() {
        assert!(AdapterKind::try_from(0).is_err());
        assert!(AdapterKind::try_from(22).is_err());
        assert!(AdapterKind::try_from(-1).is_err());
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
