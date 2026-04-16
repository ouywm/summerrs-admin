use futures::stream::BoxStream;
use rig::client::CompletionClient;
use rig::providers::{
    anthropic, cohere, deepseek, gemini, groq, moonshot, ollama, openai, openrouter, perplexity,
    together, xai,
};

#[derive(Clone)]
pub enum AnyClient {
    OpenAI(openai::Client),
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

#[derive(Debug, Clone)]
pub enum StreamChunk {
    Delta(String),
    Done,
}

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
                Err(e) => Some(Err(format!("streaming error: {e}"))),
                _ => None,
            }
        });

        Ok(Box::pin(mapped) as BoxStream<'static, Result<StreamChunk, String>>)
    }};
}

impl AnyClient {
    pub async fn stream_prompt(
        &self,
        model: &str,
        preamble: Option<&str>,
        prompt: &str,
    ) -> Result<BoxStream<'static, Result<StreamChunk, String>>, String> {
        let model = model.to_string();
        let preamble = preamble.map(|s| s.to_string());
        let prompt = prompt.to_string();

        match self {
            AnyClient::OpenAI(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::Anthropic(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::DeepSeek(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::Gemini(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::Groq(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::Ollama(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::OpenRouter(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::Perplexity(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::Together(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::XAI(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::Cohere(c) => do_stream_prompt!(c, model, preamble, prompt),
            AnyClient::Moonshot(c) => do_stream_prompt!(c, model, preamble, prompt),
        }
    }

    pub async fn prompt(
        &self,
        model: &str,
        preamble: Option<&str>,
        prompt: &str,
    ) -> Result<String, String> {
        use rig::completion::Prompt;

        let model = model.to_string();
        let preamble = preamble.map(|s| s.to_string());
        let prompt = prompt.to_string();

        macro_rules! do_prompt {
            ($client:expr) => {{
                let mut builder = $client.agent(&model);
                if let Some(ref sys) = preamble {
                    builder = builder.preamble(sys);
                }
                let agent = builder.build();
                agent
                    .prompt(&prompt)
                    .await
                    .map_err(|e| format!("prompt failed: {e}"))
            }};
        }

        match self {
            AnyClient::OpenAI(c) => do_prompt!(c),
            AnyClient::Anthropic(c) => do_prompt!(c),
            AnyClient::DeepSeek(c) => do_prompt!(c),
            AnyClient::Gemini(c) => do_prompt!(c),
            AnyClient::Groq(c) => do_prompt!(c),
            AnyClient::Ollama(c) => do_prompt!(c),
            AnyClient::OpenRouter(c) => do_prompt!(c),
            AnyClient::Perplexity(c) => do_prompt!(c),
            AnyClient::Together(c) => do_prompt!(c),
            AnyClient::XAI(c) => do_prompt!(c),
            AnyClient::Cohere(c) => do_prompt!(c),
            AnyClient::Moonshot(c) => do_prompt!(c),
        }
    }
}

#[derive(Clone)]
pub struct ProviderEntry {
    pub name: String,
    pub provider_type: String,
    pub default_model: Option<String>,
    pub client: AnyClient,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_any_client_clone() {
        let config = crate::config::ProviderConfig {
            provider_type: "ollama".into(),
            api_key: None,
            base_url: None,
            default_model: None,
        };
        let client = crate::provider::create_ollama_client(&config).unwrap();
        let any = AnyClient::Ollama(client);
        let _cloned = any.clone();
    }

    #[test]
    fn test_provider_entry_clone() {
        let config = crate::config::ProviderConfig {
            provider_type: "ollama".into(),
            api_key: None,
            base_url: None,
            default_model: None,
        };
        let client = crate::provider::create_ollama_client(&config).unwrap();
        let entry = ProviderEntry {
            name: "test".into(),
            provider_type: "ollama".into(),
            default_model: Some("llama3".into()),
            client: AnyClient::Ollama(client),
        };
        let cloned = entry.clone();
        assert_eq!(cloned.name, "test");
        assert_eq!(cloned.provider_type, "ollama");
        assert_eq!(cloned.default_model.as_deref(), Some("llama3"));
    }

    #[test]
    fn test_any_client_variants_match() {
        let ollama_config = crate::config::ProviderConfig {
            provider_type: "ollama".into(),
            api_key: None,
            base_url: None,
            default_model: None,
        };
        let ollama = crate::provider::create_ollama_client(&ollama_config).unwrap();
        let any_ollama = AnyClient::Ollama(ollama);
        assert!(matches!(any_ollama, AnyClient::Ollama(_)));

        let openai_config = crate::config::ProviderConfig {
            provider_type: "openai".into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            default_model: None,
        };
        let openai = crate::provider::create_openai_client(&openai_config).unwrap();
        let any_openai = AnyClient::OpenAI(openai);
        assert!(matches!(any_openai, AnyClient::OpenAI(_)));
        assert!(!matches!(any_openai, AnyClient::Ollama(_)));
    }
}
