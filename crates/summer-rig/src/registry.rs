use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::BoxStream;

use crate::client::{AnyClient, ProviderEntry, StreamChunk};

/// Provider 注册表组件，持有所有已配置的 provider 实例
#[derive(Clone)]
pub struct RigRegistry {
    providers: Arc<HashMap<String, ProviderEntry>>,
    default_provider: String,
}

impl RigRegistry {
    pub fn new(providers: HashMap<String, ProviderEntry>, default_provider: String) -> Self {
        Self {
            providers: Arc::new(providers),
            default_provider,
        }
    }

    /// 获取默认 provider
    pub fn default_provider(&self) -> &ProviderEntry {
        self.providers
            .get(&self.default_provider)
            .expect("默认 provider 不存在（初始化时应已校验）")
    }

    /// 获取默认 provider 的 client
    pub fn default_client(&self) -> &AnyClient {
        &self.default_provider().client
    }

    /// 按名称获取 provider
    pub fn provider(&self, name: &str) -> Option<&ProviderEntry> {
        self.providers.get(name)
    }

    /// 按名称获取 client
    pub fn client(&self, name: &str) -> Option<&AnyClient> {
        self.providers.get(name).map(|e| &e.client)
    }

    /// 返回所有已注册的 provider 名称
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(|k| k.as_str()).collect()
    }

    /// 获取默认 provider 名称
    pub fn default_provider_name(&self) -> &str {
        &self.default_provider
    }

    // ── 便利方法：直接调用，自动解析 provider + model ──

    /// 解析 provider 和 model：优先用传入值，否则走默认
    fn resolve(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Result<(&AnyClient, String), String> {
        let entry = match provider {
            Some(name) => self
                .providers
                .get(name)
                .ok_or_else(|| format!("provider '{name}' 不存在"))?,
            None => self.default_provider(),
        };
        let model = model
            .map(String::from)
            .or_else(|| entry.default_model.clone())
            .ok_or_else(|| {
                format!(
                    "provider '{}' 未配置 default_model，需显式传入 model",
                    entry.name
                )
            })?;
        Ok((&entry.client, model))
    }

    /// 流式 prompt（使用默认 provider + 默认 model）
    pub async fn stream_prompt(
        &self,
        prompt: &str,
    ) -> Result<BoxStream<'static, Result<StreamChunk, String>>, String> {
        let (client, model) = self.resolve(None, None)?;
        client.stream_prompt(&model, None, prompt).await
    }

    /// 流式 prompt（完整参数）
    pub async fn stream_prompt_with(
        &self,
        prompt: &str,
        provider: Option<&str>,
        model: Option<&str>,
        preamble: Option<&str>,
    ) -> Result<BoxStream<'static, Result<StreamChunk, String>>, String> {
        let (client, model) = self.resolve(provider, model)?;
        client.stream_prompt(&model, preamble, prompt).await
    }

    /// 非流式 prompt（使用默认 provider + 默认 model）
    pub async fn prompt(&self, prompt: &str) -> Result<String, String> {
        let (client, model) = self.resolve(None, None)?;
        client.prompt(&model, None, prompt).await
    }

    /// 非流式 prompt（完整参数）
    pub async fn prompt_with(
        &self,
        prompt: &str,
        provider: Option<&str>,
        model: Option<&str>,
        preamble: Option<&str>,
    ) -> Result<String, String> {
        let (client, model) = self.resolve(provider, model)?;
        client.prompt(&model, preamble, prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::AnyClient;
    use crate::provider::create_ollama_client;

    fn make_test_registry() -> RigRegistry {
        let ollama_config = crate::config::ProviderConfig {
            provider_type: "ollama".into(),
            api_key: None,
            base_url: None,
            default_model: Some("qwen2.5:14b".into()),
        };
        let ollama_client = create_ollama_client(&ollama_config).unwrap();

        let openai_config = crate::config::ProviderConfig {
            provider_type: "openai".into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            default_model: Some("gpt-4o".into()),
        };
        let openai_client = crate::provider::create_openai_client(&openai_config).unwrap();

        let mut providers = HashMap::new();
        providers.insert(
            "local".into(),
            ProviderEntry {
                name: "local".into(),
                provider_type: "ollama".into(),
                default_model: Some("qwen2.5:14b".into()),
                client: AnyClient::Ollama(ollama_client),
            },
        );
        providers.insert(
            "cloud".into(),
            ProviderEntry {
                name: "cloud".into(),
                provider_type: "openai".into(),
                default_model: Some("gpt-4o".into()),
                client: AnyClient::OpenAI(openai_client),
            },
        );

        RigRegistry::new(providers, "local".into())
    }

    #[test]
    fn test_default_provider() {
        let reg = make_test_registry();
        let default = reg.default_provider();
        assert_eq!(default.name, "local");
        assert_eq!(default.provider_type, "ollama");
        assert_eq!(default.default_model.as_deref(), Some("qwen2.5:14b"));
    }

    #[test]
    fn test_default_provider_name() {
        let reg = make_test_registry();
        assert_eq!(reg.default_provider_name(), "local");
    }

    #[test]
    fn test_default_client() {
        let reg = make_test_registry();
        let client = reg.default_client();
        assert!(matches!(client, AnyClient::Ollama(_)));
    }

    #[test]
    fn test_provider_by_name() {
        let reg = make_test_registry();

        let cloud = reg.provider("cloud").unwrap();
        assert_eq!(cloud.provider_type, "openai");
        assert_eq!(cloud.default_model.as_deref(), Some("gpt-4o"));

        let local = reg.provider("local").unwrap();
        assert_eq!(local.provider_type, "ollama");
    }

    #[test]
    fn test_provider_not_found() {
        let reg = make_test_registry();
        assert!(reg.provider("nonexistent").is_none());
    }

    #[test]
    fn test_client_by_name() {
        let reg = make_test_registry();
        let client = reg.client("cloud").unwrap();
        assert!(matches!(client, AnyClient::OpenAI(_)));

        assert!(reg.client("nonexistent").is_none());
    }

    #[test]
    fn test_provider_names() {
        let reg = make_test_registry();
        let mut names = reg.provider_names();
        names.sort();
        assert_eq!(names, vec!["cloud", "local"]);
    }

    #[test]
    fn test_registry_clone() {
        let reg = make_test_registry();
        let cloned = reg.clone();
        assert_eq!(cloned.default_provider_name(), "local");
        assert_eq!(cloned.provider_names().len(), 2);
    }

    #[test]
    fn test_empty_registry() {
        let reg = RigRegistry::new(HashMap::new(), "none".into());
        assert!(reg.provider_names().is_empty());
        assert!(reg.provider("any").is_none());
        assert!(reg.client("any").is_none());
    }

    #[test]
    #[should_panic(expected = "默认 provider 不存在")]
    fn test_default_provider_missing_panics() {
        let reg = RigRegistry::new(HashMap::new(), "missing".into());
        let _ = reg.default_provider();
    }
}
