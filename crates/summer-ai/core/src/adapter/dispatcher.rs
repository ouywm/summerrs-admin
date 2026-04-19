//! 中央静态分派器。
//!
//! 所有入口方法签名都是 `(kind: AdapterKind, ...)`，内部 `match kind` 调用具体
//! Adapter 的 associated fn。加新 adapter 时，每个方法的 `match` 漏写会编译失败
//! （Rust 的 exhaustive match）。
//!
//! 当前实现 `OpenAI` / `OpenAICompat` / `Anthropic`；其他变体返
//! [`AdapterError::Unsupported`]。

use bytes::Bytes;

use super::adapters::{AnthropicAdapter, GeminiAdapter, OpenAIAdapter, OpenAICompatAdapter};
use super::{
    Adapter, AdapterKind, AuthStrategy, Capabilities, CostProfile, ServiceType, WebRequestData,
};
use crate::error::{AdapterError, AdapterResult};
use crate::resolver::{AuthData, Endpoint, ServiceTarget};
use crate::types::{ChatRequest, ChatResponse, ChatStreamEvent};

/// 调度器：所有外部调用都经这里。
pub struct AdapterDispatcher;

impl AdapterDispatcher {
    // ─────────────────────── 元数据 / 默认值 ───────────────────────

    pub fn default_endpoint(kind: AdapterKind) -> Option<Endpoint> {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::default_endpoint(),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::default_endpoint(),
            AdapterKind::Anthropic => AnthropicAdapter::default_endpoint(),
            AdapterKind::Gemini => GeminiAdapter::default_endpoint(),
            _ => None,
        }
    }

    pub fn default_auth(kind: AdapterKind) -> AuthData {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::default_auth(),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::default_auth(),
            AdapterKind::Anthropic => AnthropicAdapter::default_auth(),
            AdapterKind::Gemini => GeminiAdapter::default_auth(),
            _ => match kind.default_api_key_env_name() {
                Some(env) => AuthData::from_env(env),
                None => AuthData::None,
            },
        }
    }

    pub fn capabilities(kind: AdapterKind) -> Capabilities {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::capabilities(),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::capabilities(),
            AdapterKind::Anthropic => AnthropicAdapter::capabilities(),
            AdapterKind::Gemini => GeminiAdapter::capabilities(),
            _ => Capabilities::default(),
        }
    }

    pub fn auth_strategy(kind: AdapterKind) -> AuthStrategy {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::auth_strategy(),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::auth_strategy(),
            AdapterKind::Anthropic => AnthropicAdapter::auth_strategy(),
            AdapterKind::Gemini => GeminiAdapter::auth_strategy(),
            _ => AuthStrategy::Bearer,
        }
    }

    pub fn cost_profile(kind: AdapterKind) -> CostProfile {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::cost_profile(),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::cost_profile(),
            AdapterKind::Anthropic => AnthropicAdapter::cost_profile(),
            AdapterKind::Gemini => GeminiAdapter::cost_profile(),
            _ => CostProfile::default(),
        }
    }

    // ─────────────────────── 核心转换流水线 ───────────────────────

    /// 构造上游 HTTP 请求（URL + headers + payload）。
    pub fn build_chat_request(
        kind: AdapterKind,
        target: &ServiceTarget,
        service: ServiceType,
        request: &ChatRequest,
    ) -> AdapterResult<WebRequestData> {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::build_chat_request(target, service, request),
            AdapterKind::OpenAICompat => {
                OpenAICompatAdapter::build_chat_request(target, service, request)
            }
            AdapterKind::Anthropic => {
                AnthropicAdapter::build_chat_request(target, service, request)
            }
            AdapterKind::Gemini => GeminiAdapter::build_chat_request(target, service, request),
            other => Err(unsupported(other, "chat")),
        }
    }

    /// 解析上游非流式响应 body。
    pub fn parse_chat_response(
        kind: AdapterKind,
        target: &ServiceTarget,
        body: Bytes,
    ) -> AdapterResult<ChatResponse> {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::parse_chat_response(target, body),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::parse_chat_response(target, body),
            AdapterKind::Anthropic => AnthropicAdapter::parse_chat_response(target, body),
            AdapterKind::Gemini => GeminiAdapter::parse_chat_response(target, body),
            other => Err(unsupported(other, "chat")),
        }
    }

    /// 解析上游 SSE 的单个原始事件（已去 `data: ` 前缀）。
    pub fn parse_chat_stream_event(
        kind: AdapterKind,
        target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Option<ChatStreamEvent>> {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::parse_chat_stream_event(target, raw),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::parse_chat_stream_event(target, raw),
            AdapterKind::Anthropic => AnthropicAdapter::parse_chat_stream_event(target, raw),
            AdapterKind::Gemini => GeminiAdapter::parse_chat_stream_event(target, raw),
            other => Err(unsupported(other, "chat_stream")),
        }
    }

    // ─────────────────────── 运维 / 管理面 ───────────────────────

    /// 向上游拉取可用的 model 列表（`/v1/models` 端点 + admin 连通性测试用）。
    pub async fn fetch_model_names(
        kind: AdapterKind,
        target: &ServiceTarget,
        http: &reqwest::Client,
    ) -> AdapterResult<Vec<String>> {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::fetch_model_names(target, http).await,
            AdapterKind::OpenAICompat => OpenAICompatAdapter::fetch_model_names(target, http).await,
            AdapterKind::Anthropic => AnthropicAdapter::fetch_model_names(target, http).await,
            AdapterKind::Gemini => GeminiAdapter::fetch_model_names(target, http).await,
            other => Err(unsupported(other, "fetch_model_names")),
        }
    }

    // ─────────────────────── 错误映射 ───────────────────────

    pub fn map_error(kind: AdapterKind, status: u16, body: &[u8]) -> AdapterError {
        match kind {
            AdapterKind::OpenAI => OpenAIAdapter::map_error(status, body),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::map_error(status, body),
            AdapterKind::Anthropic => AnthropicAdapter::map_error(status, body),
            AdapterKind::Gemini => GeminiAdapter::map_error(status, body),
            _ => AdapterError::UpstreamStatus {
                status,
                message: String::from_utf8_lossy(body).to_string(),
            },
        }
    }
}

// ─────────────────────── helpers ───────────────────────

fn unsupported(kind: AdapterKind, feature: &'static str) -> AdapterError {
    AdapterError::Unsupported {
        adapter: kind.as_str(),
        feature,
    }
}
