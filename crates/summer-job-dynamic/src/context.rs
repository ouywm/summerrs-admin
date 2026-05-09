//! 单次任务执行的运行时上下文。

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use summer::app::App;

pub type JobResult = Result<Value, JobError>;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("handler error: {0}")]
    Handler(#[from] anyhow::Error),

    #[error("invalid params: {0}")]
    InvalidParams(String),

    #[error("timeout after {0:?}")]
    Timeout(Duration),
}

/// `app` 是 summer 的 `Arc<App>`，handler 内部用 `app.get_expect_component::<T>()`
/// 取所需组件（DbConn / Redis / S3 等）。
#[derive(Clone)]
pub struct JobContext {
    pub run_id: i64,
    pub job_id: i64,
    pub trace_id: String,
    pub params: Value,
    pub retry_count: i32,
    pub app: Arc<App>,
}

impl JobContext {
    /// 反序列化任务参数到具体类型。失败转 `JobError::InvalidParams`。
    pub fn params_as<T: serde::de::DeserializeOwned>(&self) -> Result<T, JobError> {
        serde_json::from_value::<T>(self.params.clone())
            .map_err(|e| JobError::InvalidParams(e.to_string()))
    }

    /// 从 app 取一个 component；不存在时 panic。
    pub fn component<T: Clone + Send + Sync + 'static>(&self) -> T {
        use summer::plugin::ComponentRegistry;
        self.app.get_expect_component::<T>()
    }

    /// 从 app 取一个 component，不存在时返回 `None`。
    pub fn try_component<T: Clone + Send + Sync + 'static>(&self) -> Option<T> {
        use summer::plugin::ComponentRegistry;
        self.app.get_component::<T>()
    }

    /// 加载一段配置。失败时转 `JobError::Handler`。
    pub fn config<T>(&self) -> Result<T, JobError>
    where
        T: serde::de::DeserializeOwned + summer::config::Configurable,
    {
        use summer::config::ConfigRegistry;
        self.app
            .get_config::<T>()
            .map_err(|e| JobError::Handler(anyhow::anyhow!("加载配置失败: {e}")))
    }
}
