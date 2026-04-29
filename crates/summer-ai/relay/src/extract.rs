//! relay 层的请求提取器与通用 header 脱敏工具。
//!
//! 把四个 handler 重复的 "取 client IP / user-agent / headers / 匹配路径" 合并成
//! 一个 [`RelayRequestMeta`]，减少 handler 签名冗余。同时暴露
//! [`sanitize_headers`] 给 pipeline 脱敏出站 headers，落 tracking 表时敏感值
//! 不能明文。

use std::net::IpAddr;

use serde_json::Value;
use summer_common::extractor::ClientIp;
use summer_web::axum::extract::{FromRequestParts, MatchedPath};
use summer_web::axum::http::HeaderMap;
use summer_web::axum::http::header::USER_AGENT;
use summer_web::axum::http::request::Parts;
use summer_web::middleware::request_id::RequestId;

/// 一次 relay 请求的元数据快照 —— tracking / billing 都要填的底料。
///
/// - `request_id`：来自 request extensions 里的 `RequestId`（根路由中间件统一注入）
/// - `endpoint`：**路由模板**（如 `/v1beta/models/{target}`），不是实际 URL。
/// - `client_ip`：经 `axum_client_ip::ClientIpSource::ConnectInfo` 解析后的真实 IP 字符串
/// - `user_agent`：原始 UA 字符串，v1 不做 browser/os 解析（AI 客户端大多是 SDK）
/// - `client_headers`：入站 headers 快照（脱敏后），落 `ai.request.request_headers` JSONB
#[derive(Debug, Clone)]
pub struct RelayRequestMeta {
    pub request_id: String,
    pub endpoint: String,
    pub client_ip: String,
    pub user_agent: String,
    pub client_headers: Value,
}

impl<S> FromRequestParts<S> for RelayRequestMeta
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let endpoint = MatchedPath::from_request_parts(parts, state)
            .await
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|_| parts.uri.path().to_string());

        let client_ip = ClientIp::from_request_parts(parts, state)
            .await
            .map(|ClientIp(ip): ClientIp| fmt_ip(ip))
            .unwrap_or_default();

        let user_agent = parts
            .headers
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let client_headers = sanitize_headers(&parts.headers);
        let request_id = parts
            .extensions
            .get::<RequestId>()
            .and_then(|id| id.header_value().to_str().ok())
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .unwrap_or_else(fallback_request_id);

        Ok(Self {
            request_id,
            endpoint,
            client_ip,
            user_agent,
            client_headers,
        })
    }
}

fn fallback_request_id() -> String {
    format!("req_{}", uuid::Uuid::new_v4().simple())
}

fn fmt_ip(ip: IpAddr) -> String {
    ip.to_string()
}

/// 把 `HeaderMap` 序列化成 JSON object，同时把敏感 header 值替换为 `"<REDACTED>"`。
///
/// 命中下列 name（大小写不敏感）即脱敏：`authorization` / `proxy-authorization` /
/// `cookie` / `set-cookie` / `x-api-key` / `api-key` / `openai-organization` /
/// `anthropic-api-key` / `google-api-key`。
///
/// 同名多值按字符串数组保存；其余 header 单值字符串保存。非 UTF-8 的 header 值
/// 用 lossy 解码（极罕见，客户端基本都是 UTF-8）。
pub fn sanitize_headers(headers: &HeaderMap) -> Value {
    use serde_json::Map;

    const REDACT_LIST: &[&str] = &[
        "authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
        "api-key",
        "openai-organization",
        "anthropic-api-key",
        "google-api-key",
    ];
    const REDACTED: &str = "<REDACTED>";

    let mut map: Map<String, Value> = Map::new();
    for name in headers.keys() {
        let key = name.as_str().to_ascii_lowercase();
        let redact = REDACT_LIST.iter().any(|n| *n == key);
        let values: Vec<String> = headers
            .get_all(name)
            .iter()
            .map(|v| {
                if redact {
                    REDACTED.to_string()
                } else {
                    String::from_utf8_lossy(v.as_bytes()).to_string()
                }
            })
            .collect();
        let value = if values.len() == 1 {
            Value::String(values.into_iter().next().unwrap())
        } else {
            Value::Array(values.into_iter().map(Value::String).collect())
        };
        map.insert(key, value);
    }
    Value::Object(map)
}

/// 同 [`sanitize_headers`]，但输入是 `reqwest::header::HeaderMap`（出站请求用）。
pub fn sanitize_reqwest_headers(headers: &reqwest::header::HeaderMap) -> Value {
    use serde_json::Map;

    const REDACT_LIST: &[&str] = &[
        "authorization",
        "proxy-authorization",
        "cookie",
        "x-api-key",
        "api-key",
        "openai-organization",
        "anthropic-api-key",
        "google-api-key",
    ];
    const REDACTED: &str = "<REDACTED>";

    let mut map: Map<String, Value> = Map::new();
    for name in headers.keys() {
        let key = name.as_str().to_ascii_lowercase();
        let redact = REDACT_LIST.iter().any(|n| *n == key);
        let values: Vec<String> = headers
            .get_all(name)
            .iter()
            .map(|v| {
                if redact {
                    REDACTED.to_string()
                } else {
                    String::from_utf8_lossy(v.as_bytes()).to_string()
                }
            })
            .collect();
        let value = if values.len() == 1 {
            Value::String(values.into_iter().next().unwrap())
        } else {
            Value::Array(values.into_iter().map(Value::String).collect())
        };
        map.insert(key, value);
    }
    Value::Object(map)
}

/// 从上游 response headers 里抽常见的 request-id 值。
///
/// 检查顺序：`x-request-id` → `request-id` → `openai-request-id`
/// → `anthropic-request-id` → `x-goog-request-id`。
pub fn extract_upstream_request_id(headers: &reqwest::header::HeaderMap) -> Option<String> {
    const CANDIDATES: &[&str] = &[
        "x-request-id",
        "request-id",
        "openai-request-id",
        "anthropic-request-id",
        "x-goog-request-id",
    ];
    for name in CANDIDATES {
        if let Some(v) = headers.get(*name)
            && let Ok(s) = v.to_str()
            && !s.is_empty()
        {
            return Some(s.to_string());
        }
    }
    None
}
