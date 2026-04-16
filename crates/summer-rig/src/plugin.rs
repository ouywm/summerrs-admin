use std::collections::HashMap;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{MutableComponentRegistry, Plugin};

use crate::config::RigConfig;
use crate::factory::ProviderFactoryCatalog;
use crate::registry::{ProviderDescriptor, RigRegistry};
use crate::service::RigService;

pub struct SummerRigPlugin;

#[async_trait]
impl Plugin for SummerRigPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<RigConfig>()
            .expect("rig config section is required");

        config.validate().expect("rig config validation failed");

        let factories = ProviderFactoryCatalog::default();
        let mut descriptors = HashMap::new();
        let mut chat_backends = HashMap::new();

        for (name, provider_config) in &config.providers {
            let runtime = factories
                .create_runtime(name, provider_config)
                .unwrap_or_else(|error| panic!("create provider runtime [{name}] failed: {error}"));

            descriptors.insert(
                name.clone(),
                ProviderDescriptor {
                    name: name.clone(),
                    provider_type: runtime.provider_type,
                    backend: provider_config.backend,
                    default_model: provider_config.default_model.clone(),
                },
            );
            chat_backends.insert(name.clone(), runtime.chat_backend);
        }

        let registry = RigRegistry::new(descriptors, config.default_provider.clone())
            .expect("rig registry initialization failed");
        let service = RigService::new(registry, chat_backends);
        app.add_component(service);

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

#[cfg(test)]
mod tests {
    use super::*;
    use summer::App;
    use summer::plugin::ComponentRegistry;

    const CONFIG: &str = r#"
        [rig]
        default_provider = "default"

        [rig.providers.default]
        backend = "rig"
        provider_type = "ollama"
        base_url = "http://localhost:11434"
        default_model = "qwen2.5:14b"
    "#;

    #[tokio::test]
    async fn plugin_registers_rig_service() {
        let mut app = App::new();
        app.use_config_str(CONFIG);

        SummerRigPlugin.build(&mut app).await;

        assert!(app.has_component::<RigService>());
    }
}
