use std::collections::HashMap;
use std::sync::Arc;

use crate::backend::ChatBackendHandle;
use crate::backend::rig::RigProviderFactory;
use crate::config::{ProviderBackend, ProviderConfig};
use crate::error::RigError;

pub(crate) struct ProviderRuntime {
    pub(crate) provider_type: String,
    pub(crate) chat_backend: ChatBackendHandle,
}

pub(crate) trait ProviderFactory: Send + Sync {
    fn backend(&self) -> ProviderBackend;

    fn create_runtime(
        &self,
        provider_name: &str,
        config: &ProviderConfig,
    ) -> Result<ProviderRuntime, RigError>;
}

pub(crate) struct ProviderFactoryCatalog {
    factories: HashMap<ProviderBackend, Arc<dyn ProviderFactory>>,
}

impl Default for ProviderFactoryCatalog {
    fn default() -> Self {
        let mut factories: HashMap<ProviderBackend, Arc<dyn ProviderFactory>> = HashMap::new();
        let rig_factory: Arc<dyn ProviderFactory> = Arc::new(RigProviderFactory);
        factories.insert(rig_factory.backend(), rig_factory);
        Self { factories }
    }
}

impl ProviderFactoryCatalog {
    pub(crate) fn create_runtime(
        &self,
        provider_name: &str,
        config: &ProviderConfig,
    ) -> Result<ProviderRuntime, RigError> {
        let factory = self.factories.get(&config.backend).ok_or_else(|| {
            RigError::BackendInit(format!("unsupported backend: {}", config.backend.as_str()))
        })?;
        factory.create_runtime(provider_name, config)
    }
}
