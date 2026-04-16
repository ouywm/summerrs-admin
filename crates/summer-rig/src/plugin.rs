use std::collections::{HashMap, HashSet};

use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{MutableComponentRegistry, Plugin};

use crate::client::{AnyClient, ProviderEntry};
use crate::config::RigConfig;
use crate::provider::ProviderType;
use crate::registry::RigRegistry;

pub struct SummerRigPlugin;

#[async_trait]
impl Plugin for SummerRigPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<RigConfig>()
            .expect("rig config section is required");

        assert!(
            !config.providers.is_empty(),
            "rig.providers must not be empty"
        );
        assert!(
            config.providers.contains_key(&config.default_provider),
            "rig.default_provider = {:?} is not present in providers",
            config.default_provider
        );

        let mut entries = HashMap::new();
        let mut registered_types = HashSet::new();

        for (name, provider_config) in &config.providers {
            let provider_type = provider_config
                .provider_type
                .parse::<ProviderType>()
                .unwrap_or_else(|error| panic!("provider [{name}] config error: {error}"));

            let (client, type_label) = match provider_type {
                ProviderType::OpenAI => {
                    let client = crate::provider::create_openai_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create openai client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::OpenAI) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::OpenAI(client), "openai")
                }
                ProviderType::Anthropic => {
                    let client = crate::provider::create_anthropic_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create anthropic client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::Anthropic) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::Anthropic(client), "anthropic")
                }
                ProviderType::DeepSeek => {
                    let client = crate::provider::create_deepseek_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create deepseek client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::DeepSeek) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::DeepSeek(client), "deepseek")
                }
                ProviderType::Gemini => {
                    let client = crate::provider::create_gemini_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create gemini client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::Gemini) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::Gemini(client), "gemini")
                }
                ProviderType::Groq => {
                    let client = crate::provider::create_groq_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create groq client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::Groq) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::Groq(client), "groq")
                }
                ProviderType::Ollama => {
                    let client = crate::provider::create_ollama_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create ollama client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::Ollama) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::Ollama(client), "ollama")
                }
                ProviderType::OpenRouter => {
                    let client = crate::provider::create_openrouter_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create openrouter client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::OpenRouter) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::OpenRouter(client), "openrouter")
                }
                ProviderType::Perplexity => {
                    let client = crate::provider::create_perplexity_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create perplexity client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::Perplexity) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::Perplexity(client), "perplexity")
                }
                ProviderType::Together => {
                    let client = crate::provider::create_together_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create together client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::Together) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::Together(client), "together")
                }
                ProviderType::XAI => {
                    let client = crate::provider::create_xai_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create xai client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::XAI) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::XAI(client), "xai")
                }
                ProviderType::Cohere => {
                    let client = crate::provider::create_cohere_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create cohere client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::Cohere) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::Cohere(client), "cohere")
                }
                ProviderType::Moonshot => {
                    let client = crate::provider::create_moonshot_client(provider_config)
                        .unwrap_or_else(|error| {
                            panic!("create moonshot client [{name}] failed: {error}")
                        });
                    if registered_types.insert(ProviderType::Moonshot) {
                        app.add_component(client.clone());
                    }
                    (AnyClient::Moonshot(client), "moonshot")
                }
            };

            tracing::info!(
                "rig provider [{name}] initialized (type={type_label}){}",
                provider_config
                    .default_model
                    .as_deref()
                    .map(|model| format!(", default model: {model}"))
                    .unwrap_or_default()
            );

            entries.insert(
                name.clone(),
                ProviderEntry {
                    name: name.clone(),
                    provider_type: type_label.to_string(),
                    default_model: provider_config.default_model.clone(),
                    client,
                },
            );
        }

        let registry = RigRegistry::new(entries, config.default_provider.clone());
        app.add_component(registry);

        tracing::info!(
            "SummerRigPlugin initialized with {} providers, default: {}",
            config.providers.len(),
            config.default_provider
        );
    }

    fn name(&self) -> &str {
        "summer_rig::SummerRigPlugin"
    }
}
