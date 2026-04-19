//! 一次调用的上游目标：endpoint + auth + model (+ 额外 headers)。

use std::collections::BTreeMap;

use super::auth::AuthData;
use super::endpoint::Endpoint;

/// Relay 层构造、Adapter 消费的运行时上下文。
#[derive(Debug, Clone)]
pub struct ServiceTarget {
    pub endpoint: Endpoint,
    pub auth: AuthData,
    /// `channel.model_mapping` 应用后的实际上游模型名。
    pub actual_model: String,
    /// 额外请求头（OpenRouter `HTTP-Referer` / `X-Title` / Anthropic `anthropic-version` 等）。
    pub extra_headers: BTreeMap<String, String>,
}

impl ServiceTarget {
    /// 最简构造：给 endpoint + auth + model。
    pub fn new(endpoint: Endpoint, auth: AuthData, model: impl Into<String>) -> Self {
        Self {
            endpoint,
            auth,
            actual_model: model.into(),
            extra_headers: BTreeMap::new(),
        }
    }

    /// 常用便利：Bearer token + 完整 base_url string。
    pub fn bearer(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::new(
            Endpoint::from_owned(base_url),
            AuthData::from_single(api_key),
            model,
        )
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.insert(name.into(), value.into());
        self
    }
}
