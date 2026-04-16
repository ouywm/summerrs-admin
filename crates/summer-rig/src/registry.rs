use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::BoxStream;

use crate::client::{AnyClient, ProviderEntry, StreamChunk};

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

    pub fn default_provider(&self) -> &ProviderEntry {
        self.providers
            .get(&self.default_provider)
            .expect("default provider must exist")
    }

    pub fn default_client(&self) -> &AnyClient {
        &self.default_provider().client
    }

    pub fn provider(&self, name: &str) -> Option<&ProviderEntry> {
        self.providers.get(name)
    }

    pub fn client(&self, name: &str) -> Option<&AnyClient> {
        self.providers.get(name).map(|entry| &entry.client)
    }

    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(|key| key.as_str()).collect()
    }

    pub fn default_provider_name(&self) -> &str {
        &self.default_provider
    }

    fn resolve(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Result<(&AnyClient, String), String> {
        let entry = match provider {
            Some(name) => self
                .providers
                .get(name)
                .ok_or_else(|| format!("provider '{name}' not found"))?,
            None => self.default_provider(),
        };
        let model = model
            .map(String::from)
            .or_else(|| entry.default_model.clone())
            .ok_or_else(|| format!("provider '{}' missing default_model", entry.name))?;
        Ok((&entry.client, model))
    }

    pub async fn stream_prompt(
        &self,
        prompt: &str,
    ) -> Result<BoxStream<'static, Result<StreamChunk, String>>, String> {
        let (client, model) = self.resolve(None, None)?;
        client.stream_prompt(&model, None, prompt).await
    }

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

    pub async fn prompt(&self, prompt: &str) -> Result<String, String> {
        let (client, model) = self.resolve(None, None)?;
        client.prompt(&model, None, prompt).await
    }

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
