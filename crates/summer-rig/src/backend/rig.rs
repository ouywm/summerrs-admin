use async_trait::async_trait;
use rig::client::CompletionClient;
use rig::providers::{
    anthropic, cohere, deepseek, gemini, groq, moonshot, ollama, openai, openrouter, perplexity,
    together, xai,
};

use crate::backend::{ChatBackend, PromptStream};
use crate::config::{ProviderBackend, ProviderConfig};
use crate::error::RigError;
use crate::factory::{ProviderFactory, ProviderRuntime};
use crate::provider::{
    ProviderType, create_anthropic_client, create_cohere_client, create_deepseek_client,
    create_gemini_client, create_groq_client, create_moonshot_client, create_ollama_client,
    create_openai_client, create_openrouter_client, create_perplexity_client,
    create_together_client, create_xai_client,
};
use crate::service::StreamChunk;

enum AnyRigClient {
    OpenAI(openai::CompletionsClient),
    Anthropic(anthropic::Client),
    DeepSeek(deepseek::Client),
    Gemini(gemini::Client),
    Groq(groq::Client),
    Ollama(ollama::Client),
    OpenRouter(openrouter::Client),
    Perplexity(perplexity::Client),
    Together(together::Client),
    XAI(xai::Client),
    Cohere(cohere::Client),
    Moonshot(moonshot::Client),
}

struct RigChatBackend {
    client: AnyRigClient,
}

pub(crate) struct RigProviderFactory;

macro_rules! do_stream_prompt {
    ($client:expr, $model:expr, $preamble:expr, $prompt:expr) => {{
        use futures::StreamExt;
        use rig::agent::MultiTurnStreamItem;
        use rig::streaming::{StreamedAssistantContent, StreamingPrompt};

        let mut builder = $client.agent($model);
        if let Some(ref sys) = $preamble {
            builder = builder.preamble(sys);
        }
        let agent = builder.build();

        let stream = agent.stream_prompt($prompt).await;
        let mapped = stream.filter_map(|item| async move {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(
                    text,
                ))) => Some(Ok(StreamChunk::Delta(text.text))),
                Ok(MultiTurnStreamItem::FinalResponse(_)) => Some(Ok(StreamChunk::Done)),
                Err(error) => Some(Err(RigError::StreamFailed(error.to_string()))),
                _ => None,
            }
        });

        Ok(Box::pin(mapped) as PromptStream)
    }};
}

macro_rules! do_prompt {
    ($client:expr, $model:expr, $preamble:expr, $prompt:expr) => {{
        use rig::completion::Prompt;

        let mut builder = $client.agent($model);
        if let Some(ref sys) = $preamble {
            builder = builder.preamble(sys);
        }
        let agent = builder.build();

        agent
            .prompt($prompt)
            .await
            .map_err(|error| RigError::PromptFailed(error.to_string()))
    }};
}

#[async_trait]
impl ChatBackend for RigChatBackend {
    async fn prompt(
        &self,
        model: &str,
        preamble: Option<&str>,
        prompt: &str,
    ) -> Result<String, RigError> {
        let model = model.to_string();
        let preamble = preamble.map(ToOwned::to_owned);
        let prompt = prompt.to_string();

        match &self.client {
            AnyRigClient::OpenAI(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Anthropic(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::DeepSeek(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Gemini(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Groq(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Ollama(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::OpenRouter(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Perplexity(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Together(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::XAI(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Cohere(client) => do_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Moonshot(client) => do_prompt!(client, &model, preamble, &prompt),
        }
    }

    async fn stream_prompt(
        &self,
        model: &str,
        preamble: Option<&str>,
        prompt: &str,
    ) -> Result<PromptStream, RigError> {
        let model = model.to_string();
        let preamble = preamble.map(ToOwned::to_owned);
        let prompt = prompt.to_string();

        match &self.client {
            AnyRigClient::OpenAI(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Anthropic(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::DeepSeek(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Gemini(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Groq(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Ollama(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::OpenRouter(client) => {
                do_stream_prompt!(client, &model, preamble, &prompt)
            }
            AnyRigClient::Perplexity(client) => {
                do_stream_prompt!(client, &model, preamble, &prompt)
            }
            AnyRigClient::Together(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::XAI(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Cohere(client) => do_stream_prompt!(client, &model, preamble, &prompt),
            AnyRigClient::Moonshot(client) => do_stream_prompt!(client, &model, preamble, &prompt),
        }
    }
}

impl ProviderFactory for RigProviderFactory {
    fn backend(&self) -> ProviderBackend {
        ProviderBackend::Rig
    }

    fn create_runtime(
        &self,
        provider_name: &str,
        config: &ProviderConfig,
    ) -> Result<ProviderRuntime, RigError> {
        let provider_type = config.provider_type.parse::<ProviderType>()?;
        let client = match provider_type {
            ProviderType::OpenAI => AnyRigClient::OpenAI(create_openai_client(config)?),
            ProviderType::Anthropic => AnyRigClient::Anthropic(create_anthropic_client(config)?),
            ProviderType::DeepSeek => AnyRigClient::DeepSeek(create_deepseek_client(config)?),
            ProviderType::Gemini => AnyRigClient::Gemini(create_gemini_client(config)?),
            ProviderType::Groq => AnyRigClient::Groq(create_groq_client(config)?),
            ProviderType::Ollama => AnyRigClient::Ollama(create_ollama_client(config)?),
            ProviderType::OpenRouter => AnyRigClient::OpenRouter(create_openrouter_client(config)?),
            ProviderType::Perplexity => AnyRigClient::Perplexity(create_perplexity_client(config)?),
            ProviderType::Together => AnyRigClient::Together(create_together_client(config)?),
            ProviderType::XAI => AnyRigClient::XAI(create_xai_client(config)?),
            ProviderType::Cohere => AnyRigClient::Cohere(create_cohere_client(config)?),
            ProviderType::Moonshot => AnyRigClient::Moonshot(create_moonshot_client(config)?),
        };

        tracing::info!(
            "rig provider [{provider_name}] initialized (type={}){}",
            provider_type.as_str(),
            config
                .default_model
                .as_deref()
                .map(|model| format!(", default model: {model}"))
                .unwrap_or_default()
        );

        Ok(ProviderRuntime {
            provider_type: provider_type.as_str().to_string(),
            chat_backend: std::sync::Arc::new(RigChatBackend { client }),
        })
    }
}
