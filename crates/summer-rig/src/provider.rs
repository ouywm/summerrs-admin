use anyhow::{Result, bail};
use rig::client::Nothing;
use rig::providers::{
    anthropic, cohere, deepseek, gemini, groq, moonshot, ollama, openai, openrouter, perplexity,
    together, xai,
};
use std::str::FromStr;

use crate::config::ProviderConfig;

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

impl FromStr for ProviderType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
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
            other => bail!("unsupported provider type: {other}"),
        }
    }
}

impl ProviderType {
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
        .ok_or_else(|| anyhow::anyhow!("{provider_name} must configure api_key"))
}
