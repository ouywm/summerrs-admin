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
            .expect("rig 插件配置加载失败，请检查 [rig] 配置段");

        assert!(
            !config.providers.is_empty(),
            "rig.providers 不能为空，至少配置一个 provider"
        );
        assert!(
            config.providers.contains_key(&config.default_provider),
            "rig.default_provider = {:?} 在 providers 中不存在",
            config.default_provider
        );

        let mut entries = HashMap::new();
        let mut registered_types = HashSet::new();

        for (name, provider_config) in &config.providers {
            let provider_type = provider_config
                .provider_type
                .parse::<ProviderType>()
                .unwrap_or_else(|e| panic!("provider [{name}] 配置错误: {e}"));

            let (client, type_label) = match provider_type {
                ProviderType::OpenAI => {
                    let c = crate::provider::create_openai_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 openai client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::OpenAI) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::OpenAI(c), "openai")
                }
                ProviderType::Anthropic => {
                    let c = crate::provider::create_anthropic_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 anthropic client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::Anthropic) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::Anthropic(c), "anthropic")
                }
                ProviderType::DeepSeek => {
                    let c = crate::provider::create_deepseek_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 deepseek client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::DeepSeek) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::DeepSeek(c), "deepseek")
                }
                ProviderType::Gemini => {
                    let c = crate::provider::create_gemini_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 gemini client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::Gemini) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::Gemini(c), "gemini")
                }
                ProviderType::Groq => {
                    let c = crate::provider::create_groq_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 groq client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::Groq) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::Groq(c), "groq")
                }
                ProviderType::Ollama => {
                    let c = crate::provider::create_ollama_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 ollama client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::Ollama) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::Ollama(c), "ollama")
                }
                ProviderType::OpenRouter => {
                    let c = crate::provider::create_openrouter_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 openrouter client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::OpenRouter) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::OpenRouter(c), "openrouter")
                }
                ProviderType::Perplexity => {
                    let c = crate::provider::create_perplexity_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 perplexity client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::Perplexity) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::Perplexity(c), "perplexity")
                }
                ProviderType::Together => {
                    let c = crate::provider::create_together_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 together client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::Together) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::Together(c), "together")
                }
                ProviderType::XAI => {
                    let c = crate::provider::create_xai_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 xai client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::XAI) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::XAI(c), "xai")
                }
                ProviderType::Cohere => {
                    let c = crate::provider::create_cohere_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 cohere client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::Cohere) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::Cohere(c), "cohere")
                }
                ProviderType::Moonshot => {
                    let c = crate::provider::create_moonshot_client(provider_config)
                        .unwrap_or_else(|e| panic!("创建 moonshot client [{name}] 失败: {e}"));
                    if registered_types.insert(ProviderType::Moonshot) {
                        app.add_component(c.clone());
                    }
                    (AnyClient::Moonshot(c), "moonshot")
                }
            };

            tracing::info!(
                "rig provider [{name}] (type={type_label}) 已初始化{}",
                provider_config
                    .default_model
                    .as_deref()
                    .map(|m| format!("，默认模型: {m}"))
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
            "SummerRigPlugin 初始化完成，共 {} 个 provider，默认: {}",
            config.providers.len(),
            config.default_provider
        );
    }

    fn name(&self) -> &str {
        "summer_rig::SummerRigPlugin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer::App;
    use summer::plugin::ComponentRegistry;

    const SINGLE_PROVIDER_TOML: &str = r#"
        [rig]
        default_provider = "test-ollama"

        [rig.providers.test-ollama]
        provider_type = "ollama"
        base_url = "http://localhost:11434"
        default_model = "qwen2.5:14b"
    "#;

    const MULTI_PROVIDER_TOML: &str = r#"
        [rig]
        default_provider = "main"

        [rig.providers.main]
        provider_type = "openai"
        api_key = "sk-test-key"
        base_url = "https://api.example.com/v1"
        default_model = "gpt-4o"

        [rig.providers.local]
        provider_type = "ollama"
        base_url = "http://localhost:11434"
        default_model = "llama3"
    "#;

    async fn build_with_config(toml: &str) -> AppBuilder {
        let mut builder = App::new();
        builder.use_config_str(toml);
        SummerRigPlugin.build(&mut builder).await;
        builder
    }

    #[tokio::test]
    async fn test_plugin_single_provider() {
        let app = build_with_config(SINGLE_PROVIDER_TOML).await;

        // RigRegistry 应已注册
        let registry: RigRegistry = app.get_expect_component();
        assert_eq!(registry.default_provider_name(), "test-ollama");
        assert_eq!(registry.provider_names().len(), 1);

        let entry = registry.default_provider();
        assert_eq!(entry.provider_type, "ollama");
        assert_eq!(entry.default_model.as_deref(), Some("qwen2.5:14b"));
        assert!(matches!(entry.client, AnyClient::Ollama(_)));
    }

    #[tokio::test]
    async fn test_plugin_multi_provider() {
        let app = build_with_config(MULTI_PROVIDER_TOML).await;

        let registry: RigRegistry = app.get_expect_component();
        assert_eq!(registry.default_provider_name(), "main");
        assert_eq!(registry.provider_names().len(), 2);

        // 默认 provider 应为 openai
        let main = registry.provider("main").unwrap();
        assert_eq!(main.provider_type, "openai");
        assert!(matches!(main.client, AnyClient::OpenAI(_)));

        // local provider 应为 ollama
        let local = registry.provider("local").unwrap();
        assert_eq!(local.provider_type, "ollama");
        assert!(matches!(local.client, AnyClient::Ollama(_)));
    }

    #[tokio::test]
    async fn test_plugin_registers_typed_components() {
        let app = build_with_config(MULTI_PROVIDER_TOML).await;

        // 每种类型的第一个实例应作为直接组件注册
        assert!(
            app.has_component::<rig::providers::openai::Client>(),
            "openai::Client 应作为组件注册"
        );
        assert!(
            app.has_component::<rig::providers::ollama::Client>(),
            "ollama::Client 应作为组件注册"
        );
    }

    #[tokio::test]
    async fn test_plugin_duplicate_type_only_first_registered() {
        let toml = r#"
            [rig]
            default_provider = "primary"

            [rig.providers.primary]
            provider_type = "ollama"
            base_url = "http://gpu1:11434"
            default_model = "llama3"

            [rig.providers.secondary]
            provider_type = "ollama"
            base_url = "http://gpu2:11434"
            default_model = "qwen2.5:14b"
        "#;
        let app = build_with_config(toml).await;

        // 不应 panic（同类型多实例不会重复注册）
        assert!(app.has_component::<rig::providers::ollama::Client>());

        // RigRegistry 应持有两个实例
        let registry: RigRegistry = app.get_expect_component();
        assert_eq!(registry.provider_names().len(), 2);
        assert!(registry.provider("primary").is_some());
        assert!(registry.provider("secondary").is_some());
    }

    #[tokio::test]
    #[should_panic(expected = "providers 不能为空")]
    async fn test_plugin_empty_providers_panics() {
        let toml = r#"
            [rig]
            default_provider = "none"

            [rig.providers]
        "#;
        build_with_config(toml).await;
    }

    #[tokio::test]
    #[should_panic(expected = "在 providers 中不存在")]
    async fn test_plugin_default_not_in_providers_panics() {
        let toml = r#"
            [rig]
            default_provider = "missing"

            [rig.providers.actual]
            provider_type = "ollama"
        "#;
        build_with_config(toml).await;
    }

    #[tokio::test]
    #[should_panic(expected = "不支持的 provider 类型")]
    async fn test_plugin_unknown_provider_type_panics() {
        let toml = r#"
            [rig]
            default_provider = "bad"

            [rig.providers.bad]
            provider_type = "unknown_provider"
            api_key = "key"
        "#;
        build_with_config(toml).await;
    }

    #[tokio::test]
    #[should_panic(expected = "api_key")]
    async fn test_plugin_missing_api_key_panics() {
        let toml = r#"
            [rig]
            default_provider = "nokey"

            [rig.providers.nokey]
            provider_type = "openai"
        "#;
        build_with_config(toml).await;
    }

    #[test]
    fn test_plugin_name() {
        let plugin = SummerRigPlugin;
        assert_eq!(plugin.name(), "summer_rig::SummerRigPlugin");
    }

    // ── 真实 API 调用集成测试（需要有效配置） ──

    /// 从项目配置文件加载，用真实 API 测试 AI 回复
    const DEV_CONFIG: &str = include_str!("../../../config/app-dev.toml");

    #[tokio::test]
    async fn test_ai_completion_real_api() {
        let mut app = App::new();
        app.use_config_str(DEV_CONFIG);
        SummerRigPlugin.build(&mut app).await;

        let registry: RigRegistry = app.get_expect_component();
        let entry = registry.default_provider();

        println!("使用 provider: {} ({})", entry.name, entry.provider_type);
        println!("默认模型: {:?}", entry.default_model);

        let model_name = entry
            .default_model
            .as_deref()
            .expect("需要配置 default_model 才能运行此测试");

        // 根据 AnyClient 类型调用对应的 agent
        use rig::client::CompletionClient;
        use rig::completion::Prompt;

        let response = match &entry.client {
            AnyClient::OpenAI(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::Anthropic(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::DeepSeek(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::Ollama(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::Gemini(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::Groq(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::OpenRouter(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::Perplexity(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::Together(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::XAI(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::Cohere(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
            AnyClient::Moonshot(c) => {
                let agent = c.agent(model_name).build();
                agent.prompt("回复两个字：你好").await
            }
        };

        match response {
            Ok(text) => {
                println!("AI 回复: {text}");
                assert!(!text.is_empty(), "AI 回复不应为空");
            }
            Err(e) => {
                panic!("AI API 调用失败: {e}");
            }
        }
    }
}
