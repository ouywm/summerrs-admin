//! relay 层的请求提取器。
//!
//! 把四个 handler 重复的 "取 client IP / user-agent / 匹配路径" 合并成一个
//! [`RelayRequestMeta`]，减少 handler 签名冗余。

use std::net::IpAddr;

use summer_common::extractor::ClientIp;
use summer_web::axum::extract::{FromRequestParts, MatchedPath};
use summer_web::axum::http::header::USER_AGENT;
use summer_web::axum::http::request::Parts;

/// 一次 relay 请求的元数据快照 —— tracking / billing 都要填的底料。
///
/// - `endpoint`：**路由模板**（如 `/v1beta/models/{target}`），不是实际 URL。
///   聚合友好（按 endpoint group by 时所有 Gemini 请求聚成一组）；模型名已经在
///   `ai.request.requested_model` 字段里，不会丢信息。
/// - `client_ip`：经 `axum_client_ip::ClientIpSource::ConnectInfo` 解析后的真实 IP
///   字符串（见 `crates/app/src/main.rs` 的 layer 配置）。
/// - `user_agent`：原始 UA 字符串，v1 不做 browser/os 解析（AI 客户端大多是 SDK，
///   解析出来也是 Unknown）。
#[derive(Debug, Clone)]
pub struct RelayRequestMeta {
    pub endpoint: String,
    pub client_ip: String,
    pub user_agent: String,
}

impl<S> FromRequestParts<S> for RelayRequestMeta
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // 路由匹配模板。全局都被 relay 宏路由覆盖，一定命中；Fallback 用 uri.path()
        // 兜底（如果未来挂到未匹配 path 上也不会 panic）。
        let endpoint = MatchedPath::from_request_parts(parts, state)
            .await
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|_| parts.uri.path().to_string());

        // ClientIp 背后是 axum_client_ip，失败时 fallback 空串（日志记到 tracking）
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

        Ok(Self {
            endpoint,
            client_ip,
            user_agent,
        })
    }
}

fn fmt_ip(ip: IpAddr) -> String {
    ip.to_string()
}
