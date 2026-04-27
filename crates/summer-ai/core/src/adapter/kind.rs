//! [`AdapterKind`] —— `super::AdapterDispatcher` 的静态分派键。
//!
//! 渠道运行时的 `AdapterKind` 由 `vendor.api_style + scope` 推导得出，
//! 详见 `summer_ai_model::entity::routing::vendor::ApiStyle::adapter_kind`。

use serde::{Deserialize, Serialize};

/// `AdapterDispatcher` 静态分派使用的协议适配器键。
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
}
