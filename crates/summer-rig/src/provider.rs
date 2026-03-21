use anyhow::{Result, bail};
use rig::client::Nothing;
use rig::providers::{
    anthropic, cohere, deepseek, gemini, groq, moonshot, ollama, openai, openrouter, perplexity,
    together, xai,
};

use crate::config::ProviderConfig;

/// 支持的 Provider 类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    DeepSeek,
    Gemini,
    Groq,
    Ollama,
    OpenRouter,
    Perplexity,
    Together,
    XAI,
    Cohere,
    Moonshot,
}

impl ProviderType {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAI),
            "anthropic" => Ok(Self::Anthropic),
            "deepseek" => Ok(Self::DeepSeek),
            "gemini" | "google" => Ok(Self::Gemini),
            "groq" => Ok(Self::Groq),
            "ollama" => Ok(Self::Ollama),
            "openrouter" => Ok(Self::OpenRouter),
            "perplexity" => Ok(Self::Perplexity),
            "together" | "togetherai" => Ok(Self::Together),
            "xai" | "grok" => Ok(Self::XAI),
            "cohere" => Ok(Self::Cohere),
            "moonshot" | "kimi" => Ok(Self::Moonshot),
            other => bail!("不支持的 provider 类型: {other}"),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::DeepSeek => "deepseek",
            Self::Gemini => "gemini",
            Self::Groq => "groq",
            Self::Ollama => "ollama",
            Self::OpenRouter => "openrouter",
            Self::Perplexity => "perplexity",
            Self::Together => "together",
            Self::XAI => "xai",
            Self::Cohere => "cohere",
            Self::Moonshot => "moonshot",
        }
    }
}

// ── 各 provider 工厂函数 ──

pub fn create_openai_client(config: &ProviderConfig) -> Result<openai::Client> {
    let api_key = require_api_key(config, "openai")?;
    let mut builder = openai::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_anthropic_client(config: &ProviderConfig) -> Result<anthropic::Client> {
    let api_key = require_api_key(config, "anthropic")?;
    let mut builder = anthropic::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_deepseek_client(config: &ProviderConfig) -> Result<deepseek::Client> {
    let api_key = require_api_key(config, "deepseek")?;
    let mut builder = deepseek::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_gemini_client(config: &ProviderConfig) -> Result<gemini::Client> {
    let api_key = require_api_key(config, "gemini")?;
    let mut builder = gemini::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_groq_client(config: &ProviderConfig) -> Result<groq::Client> {
    let api_key = require_api_key(config, "groq")?;
    let mut builder = groq::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_ollama_client(config: &ProviderConfig) -> Result<ollama::Client> {
    let mut builder = ollama::Client::builder().api_key(Nothing);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_openrouter_client(config: &ProviderConfig) -> Result<openrouter::Client> {
    let api_key = require_api_key(config, "openrouter")?;
    let mut builder = openrouter::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_perplexity_client(config: &ProviderConfig) -> Result<perplexity::Client> {
    let api_key = require_api_key(config, "perplexity")?;
    let mut builder = perplexity::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_together_client(config: &ProviderConfig) -> Result<together::Client> {
    let api_key = require_api_key(config, "together")?;
    let mut builder = together::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_xai_client(config: &ProviderConfig) -> Result<xai::Client> {
    let api_key = require_api_key(config, "xai")?;
    let mut builder = xai::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_cohere_client(config: &ProviderConfig) -> Result<cohere::Client> {
    let api_key = require_api_key(config, "cohere")?;
    let mut builder = cohere::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

pub fn create_moonshot_client(config: &ProviderConfig) -> Result<moonshot::Client> {
    let api_key = require_api_key(config, "moonshot")?;
    let mut builder = moonshot::Client::builder().api_key(api_key);
    if let Some(ref url) = config.base_url {
        builder = builder.base_url(url);
    }
    Ok(builder.build()?)
}

fn require_api_key<'a>(config: &'a ProviderConfig, provider_name: &str) -> Result<&'a str> {
    config
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("{provider_name} 必须配置 api_key"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProviderType::from_str 测试 ──

    #[test]
    fn test_provider_type_from_str_all_variants() {
        let cases = [
            ("openai", ProviderType::OpenAI),
            ("anthropic", ProviderType::Anthropic),
            ("deepseek", ProviderType::DeepSeek),
            ("gemini", ProviderType::Gemini),
            ("google", ProviderType::Gemini),
            ("groq", ProviderType::Groq),
            ("ollama", ProviderType::Ollama),
            ("openrouter", ProviderType::OpenRouter),
            ("perplexity", ProviderType::Perplexity),
            ("together", ProviderType::Together),
            ("togetherai", ProviderType::Together),
            ("xai", ProviderType::XAI),
            ("grok", ProviderType::XAI),
            ("cohere", ProviderType::Cohere),
            ("moonshot", ProviderType::Moonshot),
            ("kimi", ProviderType::Moonshot),
        ];

        for (input, expected) in &cases {
            let result = ProviderType::from_str(input).unwrap();
            assert_eq!(result, *expected, "from_str({input:?}) 应为 {expected:?}");
        }
    }

    #[test]
    fn test_provider_type_case_insensitive() {
        assert_eq!(ProviderType::from_str("OpenAI").unwrap(), ProviderType::OpenAI);
        assert_eq!(ProviderType::from_str("ANTHROPIC").unwrap(), ProviderType::Anthropic);
        assert_eq!(ProviderType::from_str("DeepSeek").unwrap(), ProviderType::DeepSeek);
        assert_eq!(ProviderType::from_str("OLLAMA").unwrap(), ProviderType::Ollama);
    }

    #[test]
    fn test_provider_type_unknown() {
        assert!(ProviderType::from_str("unknown").is_err());
        assert!(ProviderType::from_str("").is_err());
        assert!(ProviderType::from_str("chatgpt").is_err());
    }

    #[test]
    fn test_provider_type_as_str_roundtrip() {
        let all = [
            ProviderType::OpenAI,
            ProviderType::Anthropic,
            ProviderType::DeepSeek,
            ProviderType::Gemini,
            ProviderType::Groq,
            ProviderType::Ollama,
            ProviderType::OpenRouter,
            ProviderType::Perplexity,
            ProviderType::Together,
            ProviderType::XAI,
            ProviderType::Cohere,
            ProviderType::Moonshot,
        ];
        for pt in &all {
            let s = pt.as_str();
            let back = ProviderType::from_str(s).unwrap();
            assert_eq!(*pt, back, "as_str -> from_str 往返失败: {s}");
        }
    }

    // ── 工厂函数测试 ──

    fn make_config(api_key: Option<&str>, base_url: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            provider_type: String::new(),
            api_key: api_key.map(String::from),
            base_url: base_url.map(String::from),
            default_model: None,
        }
    }

    #[test]
    fn test_create_openai_client_success() {
        let config = make_config(Some("sk-test"), Some("https://api.example.com/v1"));
        let client = create_openai_client(&config);
        assert!(client.is_ok(), "创建 openai client 应成功");
    }

    #[test]
    fn test_create_openai_client_no_key() {
        let config = make_config(None, None);
        let err = create_openai_client(&config);
        assert!(err.is_err());
        assert!(
            err.unwrap_err().to_string().contains("api_key"),
            "错误信息应包含 api_key"
        );
    }

    #[test]
    fn test_create_anthropic_client_success() {
        let config = make_config(Some("sk-ant-test"), None);
        assert!(create_anthropic_client(&config).is_ok());
    }

    #[test]
    fn test_create_deepseek_client_success() {
        let config = make_config(Some("sk-ds"), None);
        assert!(create_deepseek_client(&config).is_ok());
    }

    #[test]
    fn test_create_gemini_client_success() {
        let config = make_config(Some("AIzaSy-test"), None);
        assert!(create_gemini_client(&config).is_ok());
    }

    #[test]
    fn test_create_groq_client_success() {
        let config = make_config(Some("gsk-test"), None);
        assert!(create_groq_client(&config).is_ok());
    }

    #[test]
    fn test_create_ollama_client_no_key_required() {
        let config = make_config(None, Some("http://localhost:11434"));
        assert!(create_ollama_client(&config).is_ok());
    }

    #[test]
    fn test_create_ollama_client_default_url() {
        let config = make_config(None, None);
        assert!(create_ollama_client(&config).is_ok());
    }

    #[test]
    fn test_create_openrouter_client_success() {
        let config = make_config(Some("sk-or-test"), None);
        assert!(create_openrouter_client(&config).is_ok());
    }

    #[test]
    fn test_create_perplexity_client_success() {
        let config = make_config(Some("ppl-test"), None);
        assert!(create_perplexity_client(&config).is_ok());
    }

    #[test]
    fn test_create_together_client_success() {
        let config = make_config(Some("tog-test"), None);
        assert!(create_together_client(&config).is_ok());
    }

    #[test]
    fn test_create_xai_client_success() {
        let config = make_config(Some("xai-test"), None);
        assert!(create_xai_client(&config).is_ok());
    }

    #[test]
    fn test_create_cohere_client_success() {
        let config = make_config(Some("co-test"), None);
        assert!(create_cohere_client(&config).is_ok());
    }

    #[test]
    fn test_create_moonshot_client_success() {
        let config = make_config(Some("sk-moon"), None);
        assert!(create_moonshot_client(&config).is_ok());
    }

    #[test]
    fn test_all_keyed_providers_fail_without_key() {
        let config = make_config(None, None);
        assert!(create_openai_client(&config).is_err());
        assert!(create_anthropic_client(&config).is_err());
        assert!(create_deepseek_client(&config).is_err());
        assert!(create_gemini_client(&config).is_err());
        assert!(create_groq_client(&config).is_err());
        assert!(create_openrouter_client(&config).is_err());
        assert!(create_perplexity_client(&config).is_err());
        assert!(create_together_client(&config).is_err());
        assert!(create_xai_client(&config).is_err());
        assert!(create_cohere_client(&config).is_err());
        assert!(create_moonshot_client(&config).is_err());
        // ollama 不需要 key，不在此列
    }

    #[test]
    fn test_create_client_with_custom_base_url() {
        let config = make_config(Some("sk-test"), Some("https://proxy.example.com/v1"));
        assert!(create_openai_client(&config).is_ok());

        let config = make_config(Some("sk-test"), Some("https://claude-proxy.example.com"));
        assert!(create_anthropic_client(&config).is_ok());
    }
}
