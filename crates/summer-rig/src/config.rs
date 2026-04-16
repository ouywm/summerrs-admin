use std::collections::HashMap;

use serde::Deserialize;
use summer::config::Configurable;

#[derive(Debug, Clone, Deserialize, Configurable)]
#[config_prefix = "rig"]
pub struct RigConfig {
    pub default_provider: String,
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub provider_type: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer::App;
    use summer::config::ConfigRegistry;

    fn load_config(toml_str: &str) -> RigConfig {
        let mut builder = App::new();
        builder.use_config_str(toml_str);
        builder.get_config::<RigConfig>().expect("rig config")
    }

    #[test]
    fn test_deserialize_full_config() {
        let toml_str = r#"
            [rig]
            default_provider = "openai"

            [rig.providers.openai]
            provider_type = "openai"
            api_key = "sk-test"
            base_url = "https://api.openai.com/v1"
            default_model = "gpt-4o"

            [rig.providers.local]
            provider_type = "ollama"
            base_url = "http://localhost:11434"
            default_model = "qwen2.5:14b"
        "#;

        let config = load_config(toml_str);

        assert_eq!(config.default_provider, "openai");
        assert_eq!(config.providers.len(), 2);

        let openai = config.providers.get("openai").unwrap();
        assert_eq!(openai.provider_type, "openai");
        assert_eq!(openai.api_key.as_deref(), Some("sk-test"));
        assert_eq!(
            openai.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(openai.default_model.as_deref(), Some("gpt-4o"));

        let local = config.providers.get("local").unwrap();
        assert_eq!(local.provider_type, "ollama");
        assert!(local.api_key.is_none());
        assert_eq!(local.base_url.as_deref(), Some("http://localhost:11434"));
        assert_eq!(local.default_model.as_deref(), Some("qwen2.5:14b"));
    }

    #[test]
    fn test_deserialize_minimal_config() {
        let toml_str = r#"
            [rig]
            default_provider = "ds"

            [rig.providers.ds]
            provider_type = "deepseek"
            api_key = "sk-xxx"
        "#;

        let config = load_config(toml_str);

        assert_eq!(config.default_provider, "ds");
        assert_eq!(config.providers.len(), 1);

        let ds = config.providers.get("ds").unwrap();
        assert_eq!(ds.provider_type, "deepseek");
        assert!(ds.base_url.is_none());
        assert!(ds.default_model.is_none());
    }

    #[test]
    fn test_deserialize_multi_provider_config() {
        let toml_str = r#"
            [rig]
            default_provider = "main"

            [rig.providers.main]
            provider_type = "openai"
            api_key = "sk-1"
            default_model = "gpt-4o"

            [rig.providers.backup]
            provider_type = "anthropic"
            api_key = "sk-ant-1"
            default_model = "claude-sonnet-4-20250514"

            [rig.providers.search]
            provider_type = "perplexity"
            api_key = "ppl-1"

            [rig.providers.local]
            provider_type = "ollama"
            base_url = "http://gpu-box:11434"
            default_model = "llama3:70b"
        "#;

        let config = load_config(toml_str);

        assert_eq!(config.providers.len(), 4);
        assert!(config.providers.contains_key("main"));
        assert!(config.providers.contains_key("backup"));
        assert!(config.providers.contains_key("search"));
        assert!(config.providers.contains_key("local"));
    }
}
