use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::BoxStream;

use crate::backend::ChatBackendHandle;
use crate::config::ProviderBackend;
use crate::error::RigError;
use crate::registry::RigRegistry;

pub type PromptStream = BoxStream<'static, Result<StreamChunk, RigError>>;

#[derive(Debug, Clone, Copy, Default)]
pub struct PromptOptions<'a> {
    pub provider: Option<&'a str>,
    pub model: Option<&'a str>,
    pub preamble: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamChunk {
    Delta(String),
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub name: String,
    pub provider_type: String,
    pub backend: ProviderBackend,
    pub default_model: Option<String>,
}

#[derive(Clone)]
pub struct RigService {
    registry: Arc<RigRegistry>,
    chat_backends: Arc<HashMap<String, ChatBackendHandle>>,
}

impl RigService {
    pub(crate) fn new(
        registry: RigRegistry,
        chat_backends: HashMap<String, ChatBackendHandle>,
    ) -> Self {
        Self {
            registry: Arc::new(registry),
            chat_backends: Arc::new(chat_backends),
        }
    }

    pub async fn prompt(&self, prompt: &str) -> Result<String, RigError> {
        self.prompt_with(prompt, PromptOptions::default()).await
    }

    pub async fn prompt_with(
        &self,
        prompt: &str,
        options: PromptOptions<'_>,
    ) -> Result<String, RigError> {
        let resolved = self.registry.resolve(options.provider, options.model)?;
        let backend = self.chat_backends.get(&resolved.name).ok_or_else(|| {
            RigError::BackendInit(format!("missing backend for provider '{}'", resolved.name))
        })?;

        backend
            .prompt(&resolved.model, options.preamble, prompt)
            .await
    }

    pub async fn stream_prompt(&self, prompt: &str) -> Result<PromptStream, RigError> {
        self.stream_prompt_with(prompt, PromptOptions::default())
            .await
    }

    pub async fn stream_prompt_with(
        &self,
        prompt: &str,
        options: PromptOptions<'_>,
    ) -> Result<PromptStream, RigError> {
        let resolved = self.registry.resolve(options.provider, options.model)?;
        let backend = self.chat_backends.get(&resolved.name).ok_or_else(|| {
            RigError::BackendInit(format!("missing backend for provider '{}'", resolved.name))
        })?;

        backend
            .stream_prompt(&resolved.model, options.preamble, prompt)
            .await
    }

    pub fn provider_names(&self) -> Vec<&str> {
        self.registry.provider_names()
    }

    pub fn default_provider_name(&self) -> &str {
        self.registry.default_provider_name()
    }

    pub fn provider_metadata(&self, name: &str) -> Option<ProviderMetadata> {
        self.registry
            .descriptor(name)
            .map(|descriptor| ProviderMetadata {
                name: descriptor.name.clone(),
                provider_type: descriptor.provider_type.clone(),
                backend: descriptor.backend,
                default_model: descriptor.default_model.clone(),
            })
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use futures::stream;

    use super::*;
    use crate::backend::{ChatBackend, PromptStream as BackendPromptStream};
    use crate::registry::ProviderDescriptor;

    struct FakeBackend {
        name: &'static str,
    }

    #[async_trait]
    impl ChatBackend for FakeBackend {
        async fn prompt(
            &self,
            model: &str,
            preamble: Option<&str>,
            prompt: &str,
        ) -> Result<String, RigError> {
            match preamble {
                Some(preamble) => Ok(format!("{}:{model}:{preamble}:{prompt}", self.name)),
                None => Ok(format!("{}:{model}:{prompt}", self.name)),
            }
        }

        async fn stream_prompt(
            &self,
            model: &str,
            preamble: Option<&str>,
            prompt: &str,
        ) -> Result<BackendPromptStream, RigError> {
            let content = match preamble {
                Some(preamble) => format!("{}:{model}:{preamble}:{prompt}", self.name),
                None => format!("{}:{model}:{prompt}", self.name),
            };

            Ok(Box::pin(stream::iter(vec![
                Ok(StreamChunk::Delta(content)),
                Ok(StreamChunk::Done),
            ])))
        }
    }

    fn descriptor(
        name: &str,
        provider_type: &str,
        default_model: Option<&str>,
    ) -> ProviderDescriptor {
        ProviderDescriptor {
            name: name.to_string(),
            provider_type: provider_type.to_string(),
            backend: ProviderBackend::Rig,
            default_model: default_model.map(ToOwned::to_owned),
        }
    }

    fn service() -> RigService {
        let registry = RigRegistry::new(
            HashMap::from([
                (
                    "default".to_string(),
                    descriptor("default", "openai", Some("default-model")),
                ),
                (
                    "backup".to_string(),
                    descriptor("backup", "anthropic", Some("backup-model")),
                ),
            ]),
            "default".to_string(),
        )
        .unwrap();
        let backends: HashMap<String, ChatBackendHandle> = HashMap::from([
            (
                "default".to_string(),
                Arc::new(FakeBackend { name: "default" }) as ChatBackendHandle,
            ),
            (
                "backup".to_string(),
                Arc::new(FakeBackend { name: "backup" }) as ChatBackendHandle,
            ),
        ]);

        RigService::new(registry, backends)
    }

    #[tokio::test]
    async fn prompt_uses_default_provider_and_model() {
        let service = service();

        let response = service.prompt("hello").await.unwrap();

        assert_eq!(response, "default:default-model:hello");
    }

    #[tokio::test]
    async fn prompt_with_uses_named_provider_and_override_model() {
        let service = service();

        let response = service
            .prompt_with(
                "hello",
                PromptOptions {
                    provider: Some("backup"),
                    model: Some("gpt-4.1"),
                    preamble: Some("system"),
                },
            )
            .await
            .unwrap();

        assert_eq!(response, "backup:gpt-4.1:system:hello");
    }

    #[tokio::test]
    async fn stream_prompt_uses_same_resolution_rules() {
        use futures::StreamExt;

        let service = service();
        let mut stream = service
            .stream_prompt_with(
                "hello",
                PromptOptions {
                    provider: Some("backup"),
                    model: None,
                    preamble: Some("system"),
                },
            )
            .await
            .unwrap();

        let first = stream.next().await.unwrap().unwrap();
        let second = stream.next().await.unwrap().unwrap();

        assert_eq!(
            first,
            StreamChunk::Delta("backup:backup-model:system:hello".to_string())
        );
        assert_eq!(second, StreamChunk::Done);
    }

    #[tokio::test]
    async fn prompt_with_unknown_provider_fails() {
        let service = service();
        let error = service
            .prompt_with(
                "hello",
                PromptOptions {
                    provider: Some("missing"),
                    model: None,
                    preamble: None,
                },
            )
            .await
            .unwrap_err();

        assert_eq!(error, RigError::ProviderNotFound("missing".to_string()));
    }

    #[test]
    fn metadata_methods_expose_registry_information() {
        let service = service();

        let mut names = service.provider_names();
        names.sort();

        assert_eq!(names, vec!["backup", "default"]);
        assert_eq!(service.default_provider_name(), "default");
        assert_eq!(
            service.provider_metadata("backup"),
            Some(ProviderMetadata {
                name: "backup".to_string(),
                provider_type: "anthropic".to_string(),
                backend: ProviderBackend::Rig,
                default_model: Some("backup-model".to_string()),
            })
        );
    }
}
