use std::collections::HashMap;

use crate::config::ProviderBackend;
use crate::error::RigError;

#[derive(Debug, Clone)]
pub(crate) struct ProviderDescriptor {
    pub(crate) name: String,
    pub(crate) provider_type: String,
    pub(crate) backend: ProviderBackend,
    pub(crate) default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedProvider {
    pub(crate) name: String,
    pub(crate) model: String,
}

#[derive(Clone)]
pub(crate) struct RigRegistry {
    providers: HashMap<String, ProviderDescriptor>,
    default_provider: String,
}

impl RigRegistry {
    pub(crate) fn new(
        providers: HashMap<String, ProviderDescriptor>,
        default_provider: String,
    ) -> Result<Self, RigError> {
        if !providers.contains_key(&default_provider) {
            return Err(RigError::Config(format!(
                "rig.default_provider '{}' is not present in providers",
                default_provider
            )));
        }

        Ok(Self {
            providers,
            default_provider,
        })
    }

    pub(crate) fn resolve(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Result<ResolvedProvider, RigError> {
        let descriptor = match provider {
            Some(name) => self
                .providers
                .get(name)
                .ok_or_else(|| RigError::ProviderNotFound(name.to_string()))?,
            None => self
                .providers
                .get(&self.default_provider)
                .expect("default provider already validated"),
        };

        let model = model
            .map(ToOwned::to_owned)
            .or_else(|| descriptor.default_model.clone())
            .ok_or_else(|| RigError::DefaultModelMissing(descriptor.name.clone()))?;

        Ok(ResolvedProvider {
            name: descriptor.name.clone(),
            model,
        })
    }

    pub(crate) fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(String::as_str).collect()
    }

    pub(crate) fn descriptor(&self, name: &str) -> Option<&ProviderDescriptor> {
        self.providers.get(name)
    }

    pub(crate) fn default_provider_name(&self) -> &str {
        &self.default_provider
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(name: &str, model: Option<&str>) -> ProviderDescriptor {
        ProviderDescriptor {
            name: name.to_string(),
            provider_type: "openai".to_string(),
            backend: ProviderBackend::Rig,
            default_model: model.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn resolve_uses_default_provider_model() {
        let registry = RigRegistry::new(
            HashMap::from([("default".to_string(), descriptor("default", Some("gpt-4o")))]),
            "default".to_string(),
        )
        .unwrap();

        let resolved = registry.resolve(None, None).unwrap();

        assert_eq!(
            resolved,
            ResolvedProvider {
                name: "default".to_string(),
                model: "gpt-4o".to_string(),
            }
        );
    }

    #[test]
    fn resolve_rejects_missing_default_model() {
        let registry = RigRegistry::new(
            HashMap::from([("default".to_string(), descriptor("default", None))]),
            "default".to_string(),
        )
        .unwrap();

        let error = registry.resolve(None, None).unwrap_err();
        assert_eq!(error, RigError::DefaultModelMissing("default".to_string()));
    }
}
