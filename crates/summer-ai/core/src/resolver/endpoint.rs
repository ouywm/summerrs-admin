//! 上游 endpoint URL 的轻量封装。
//!
//! Adapter 用 `Endpoint::from_static(BASE_URL)` 声明协议默认 URL；Relay 运行时
//! 从 DB 读 channel 后用 [`Endpoint::from_owned`] 构造一次性 Endpoint 传入
//! [`ServiceTarget`](crate::ServiceTarget)。

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Endpoint(Arc<str>);

impl Endpoint {
    /// Adapter 声明协议默认 URL 用。
    pub fn from_static(url: &'static str) -> Self {
        Self(Arc::from(url))
    }

    /// Relay 层从 channel.base_url 构造。
    pub fn from_owned(url: impl Into<String>) -> Self {
        Self(Arc::from(url.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 去掉末尾 `/` 后的 URL（方便 Adapter 拼路径）。
    pub fn trimmed(&self) -> &str {
        self.0.trim_end_matches('/')
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
