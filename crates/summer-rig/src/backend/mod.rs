use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::error::RigError;
use crate::service::StreamChunk;

pub(crate) mod rig;

pub(crate) type PromptStream = BoxStream<'static, Result<StreamChunk, RigError>>;
pub(crate) type ChatBackendHandle = Arc<dyn ChatBackend>;

#[async_trait]
pub(crate) trait ChatBackend: Send + Sync {
    async fn prompt(
        &self,
        model: &str,
        preamble: Option<&str>,
        prompt: &str,
    ) -> Result<String, RigError>;

    async fn stream_prompt(
        &self,
        model: &str,
        preamble: Option<&str>,
        prompt: &str,
    ) -> Result<PromptStream, RigError>;
}
