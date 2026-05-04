use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use summer::app::App;
use tokio_util::sync::CancellationToken;

pub type JobResult = Result<Value, JobError>;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("handler error: {0}")]
    Handler(#[from] anyhow::Error),

    #[error("invalid params: {0}")]
    InvalidParams(String),

    #[error("timeout after {0:?}")]
    Timeout(Duration),

    #[error("canceled")]
    Canceled,
}

/// 单次任务执行的运行时上下文。
///
/// `app` 是 summer 的 `Arc<App>`，handler 内部用 `app.get_expect_component::<T>()`
/// 取所需组件（DbConn / Redis / S3 等）。后续 P2 会引入 `FromApp` 风格 extractor，
/// 让 handler 直接以 `Component<T>` `Config<T>` 形参声明依赖。
#[derive(Clone)]
pub struct JobContext {
    pub run_id: i64,
    pub job_id: i64,
    pub trace_id: String,
    pub params: Value,
    pub retry_count: i32,
    pub cancel: CancellationToken,
    pub app: Arc<App>,
    /// 脚本任务源码（仅 handler = `script::rhai` 等脚本引擎使用）
    pub script: Option<String>,
}

impl JobContext {
    /// 反序列化任务参数到具体类型。失败转 `JobError::InvalidParams`。
    pub fn params_as<T: serde::de::DeserializeOwned>(&self) -> Result<T, JobError> {
        serde_json::from_value::<T>(self.params.clone())
            .map_err(|e| JobError::InvalidParams(e.to_string()))
    }

    /// 长循环里调用，被取消时立即返回 `Err(JobError::Canceled)`。
    pub fn check_cancel(&self) -> Result<(), JobError> {
        if self.cancel.is_cancelled() {
            Err(JobError::Canceled)
        } else {
            Ok(())
        }
    }

    /// 从 app 取一个 component；不存在时 panic（业务通常用此变体，配置错就早崩）。
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
