//! 一次调用的上游目标：endpoint + auth + model (+ 额外 headers)。

use std::collections::BTreeMap;
use std::fmt;

use crate::adapter::AdapterKind;

use super::auth::AuthData;
use super::endpoint::Endpoint;

/// 模型身份：adapter 协议族 + 上游实际模型名。
///
/// 把 `(AdapterKind, actual_model)` 收拢到一处后，pipeline 里不必再
/// 到处传 `(kind, target)` tuple，adapter 从 `target.model.kind` 取 kind、
/// `target.model.name` 取模型名即可。
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ModelIden {
    /// 上游协议家族。
    pub kind: AdapterKind,
    /// `channel.model_mapping` 应用后发给上游的实际模型名。
    pub name: String,
}

impl ModelIden {
    pub fn new(kind: AdapterKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
        }
    }
}

impl fmt::Display for ModelIden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:?})", self.name, self.kind)
    }
}

/// Relay 层构造、Adapter 消费的运行时上下文。
#[derive(Debug, Clone)]
pub struct ServiceTarget {
    pub endpoint: Endpoint,
    pub auth: AuthData,
    /// 上游模型身份（kind + 实际模型名）。
    pub model: ModelIden,
    /// 额外请求头（OpenRouter `HTTP-Referer` / `X-Title` / Claude `anthropic-version` 等）。
    pub extra_headers: BTreeMap<String, String>,
}

impl ServiceTarget {
    /// 最简构造：给 endpoint + auth + model（kind + name）。
    pub fn new(endpoint: Endpoint, auth: AuthData, model: ModelIden) -> Self {
        Self {
            endpoint,
            auth,
            model,
            extra_headers: BTreeMap::new(),
        }
    }

    /// 常用便利：Bearer token + 完整 base_url string + (kind, model_name)。
    pub fn bearer(
        kind: AdapterKind,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model_name: impl Into<String>,
    ) -> Self {
        Self::new(
            Endpoint::from_owned(base_url),
            AuthData::from_single(api_key),
            ModelIden::new(kind, model_name),
        )
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.insert(name.into(), value.into());
        self
    }

    /// `target.model.name` 的字符串视图，给日志、tracing、wire payload 用。
    pub fn actual_model(&self) -> &str {
        &self.model.name
    }

    /// `target.model.kind` 的便利访问。
    pub fn kind(&self) -> AdapterKind {
        self.model.kind
    }
}
