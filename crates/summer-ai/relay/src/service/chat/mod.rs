use std::time::Instant;

use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_core::provider::{
    ChatProvider, ProviderErrorInfo, ProviderErrorKind, ProviderKind, ProviderRegistry,
};
use summer_ai_core::types::chat::{ChatCompletionRequest, ChatCompletionResponse};
use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel::{ChannelStatus, ChannelType};
use summer_ai_model::entity::channel_account::{self, ChannelAccountStatus};
use summer_ai_model::entity::request;
use summer_ai_model::entity::request_execution;
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::response::Json;
use summer_sea_orm::DbConn;
use summer_web::axum::body::Body;
use summer_web::axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use summer_web::axum::http::{HeaderMap, HeaderValue, StatusCode};
use summer_web::axum::response::{IntoResponse, Response};
use uuid::Uuid;

use self::stream::{TrackedChatSseStream, TrackedChatSseStreamArgs};
use crate::service::token::TokenInfo;
use crate::service::tracking::TrackingService;

mod stream;

#[derive(Clone, Service)]
pub struct ChatRelayService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    client: reqwest::Client,
    #[inject(component)]
    tracking: TrackingService,
}

impl ChatRelayService {
    pub async fn relay(
        &self,
        ctx: RelayChatContext,
        request: ChatCompletionRequest,
    ) -> Result<Response, OpenAiErrorResponse> {
        let PreparedChatRelay {
            request_id,
            started_at,
            tracked_request,
            tracked_execution,
            target,
            provider,
            request_builder,
        } = self.prepare_chat_relay(&ctx, &request).await?;

        if request.stream {
            let upstream_response = match self
                .send_upstream_chat_stream(request_builder, target.provider_kind)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    return Err(self
                        .finish_with_error(
                            &self.tracking,
                            tracked_request.as_ref(),
                            tracked_execution.as_ref(),
                            Some(&target.upstream_model),
                            None,
                            error,
                            started_at.elapsed().as_millis() as i32,
                        )
                        .await);
                }
            };

            let (response_status_code, upstream_request_id, response) = match upstream_response {
                UpstreamChatStreamResponse::Success {
                    status_code,
                    upstream_request_id,
                    response,
                } => (status_code, upstream_request_id, response),
                UpstreamChatStreamResponse::Failure {
                    upstream_request_id,
                    error,
                } => {
                    return Err(self
                        .finish_with_error(
                            &self.tracking,
                            tracked_request.as_ref(),
                            tracked_execution.as_ref(),
                            Some(&target.upstream_model),
                            upstream_request_id.as_deref(),
                            error,
                            started_at.elapsed().as_millis() as i32,
                        )
                        .await);
                }
            };

            let chunk_stream = match provider.parse_chat_stream(response, &target.upstream_model) {
                Ok(stream) => stream,
                Err(error) => {
                    return Err(self
                        .finish_with_error(
                            &self.tracking,
                            tracked_request.as_ref(),
                            tracked_execution.as_ref(),
                            Some(&target.upstream_model),
                            upstream_request_id.as_deref(),
                            OpenAiErrorResponse::internal_with(
                                "failed to parse upstream chat stream",
                                error,
                            ),
                            started_at.elapsed().as_millis() as i32,
                        )
                        .await);
                }
            };

            let stream = TrackedChatSseStream::new(TrackedChatSseStreamArgs {
                inner: chunk_stream,
                tracking: Some(self.tracking.clone()),
                tracked_request_id: tracked_request.as_ref().map(|model| model.id),
                tracked_execution_id: tracked_execution.as_ref().map(|model| model.id),
                request_id: request_id.clone(),
                started_at,
                upstream_model: target.upstream_model.clone(),
                upstream_request_id: upstream_request_id.clone(),
                response_status_code,
            });

            let mut response = Response::builder()
                .status(StatusCode::OK)
                .body(Body::from_stream(stream))
                .expect("chat stream response");
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            if let Ok(value) = HeaderValue::from_str(&request_id) {
                response.headers_mut().insert("x-request-id", value);
            }
            if let Some(upstream_request_id) = upstream_request_id
                && let Ok(value) = HeaderValue::from_str(&upstream_request_id)
            {
                response
                    .headers_mut()
                    .insert("x-upstream-request-id", value);
            }

            return Ok(response);
        }

        let upstream_response = match self
            .send_upstream_chat(request_builder, target.provider_kind)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return Err(self
                    .finish_with_error(
                        &self.tracking,
                        tracked_request.as_ref(),
                        tracked_execution.as_ref(),
                        Some(&target.upstream_model),
                        None,
                        error,
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        if let Some(error) = upstream_response.error {
            return Err(self
                .finish_with_error(
                    &self.tracking,
                    tracked_request.as_ref(),
                    tracked_execution.as_ref(),
                    Some(&target.upstream_model),
                    upstream_response.upstream_request_id.as_deref(),
                    error,
                    started_at.elapsed().as_millis() as i32,
                )
                .await);
        }

        let chat_response =
            match provider.parse_chat_response(upstream_response.body, &target.upstream_model) {
                Ok(response) => response,
                Err(error) => {
                    return Err(self
                        .finish_with_error(
                            &self.tracking,
                            tracked_request.as_ref(),
                            tracked_execution.as_ref(),
                            Some(&target.upstream_model),
                            upstream_response.upstream_request_id.as_deref(),
                            OpenAiErrorResponse::internal_with(
                                "failed to parse upstream chat response",
                                error,
                            ),
                            started_at.elapsed().as_millis() as i32,
                        )
                        .await);
                }
            };

        let duration_ms = started_at.elapsed().as_millis() as i32;
        self.try_finish_request_success(
            &self.tracking,
            tracked_request.as_ref(),
            &target.upstream_model,
            upstream_response.status_code,
            &chat_response,
            duration_ms,
        )
        .await;
        self.try_finish_execution_success(
            &self.tracking,
            tracked_execution.as_ref(),
            upstream_response.upstream_request_id.as_deref(),
            upstream_response.status_code,
            &chat_response,
            duration_ms,
        )
        .await;

        Ok(Json::<ChatCompletionResponse>(chat_response).into_response())
    }

    async fn prepare_chat_relay(
        &self,
        ctx: &RelayChatContext,
        request: &ChatCompletionRequest,
    ) -> Result<PreparedChatRelay, OpenAiErrorResponse> {
        ctx.token_info
            .ensure_endpoint_allowed("chat")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        ctx.token_info
            .ensure_model_allowed(&request.model)
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;

        let request_id = format!("req_{}", Uuid::new_v4().simple());
        let started_at = Instant::now();

        let tracked_request = match self
            .tracking
            .create_chat_request(
                &request_id,
                &ctx.token_info,
                request,
                &ctx.client_ip,
                &ctx.user_agent,
                &ctx.request_headers,
            )
            .await
        {
            Ok(model) => Some(model),
            Err(error) => {
                tracing::warn!(request_id, error = %error, "failed to create request tracking row");
                None
            }
        };

        let target = match self.resolve_target(&ctx.token_info.group, request).await {
            Ok(target) => target,
            Err(error) => {
                let openai_error = match error {
                    ApiErrors::NotFound(message) => {
                        OpenAiErrorResponse::model_not_available(message)
                    }
                    other => OpenAiErrorResponse::from_api_error(&other),
                };
                return Err(self
                    .finish_with_error(
                        &self.tracking,
                        tracked_request.as_ref(),
                        None,
                        None,
                        None,
                        openai_error,
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        let tracked_execution = if let Some(tracked_request) = tracked_request.as_ref() {
            let upstream_body = build_tracking_upstream_body(request, &target.upstream_model);
            match self
                .tracking
                .create_chat_execution(
                    tracked_request.id,
                    &request_id,
                    request,
                    target.channel.id,
                    target.account.id,
                    &target.upstream_model,
                    upstream_body,
                )
                .await
            {
                Ok(model) => Some(model),
                Err(error) => {
                    tracing::warn!(request_id, error = %error, "failed to create request_execution tracking row");
                    None
                }
            }
        } else {
            None
        };

        let provider = ProviderRegistry::chat(target.provider_kind).ok_or_else(|| {
            OpenAiErrorResponse::unsupported_endpoint("chat endpoint is disabled")
        })?;

        let request_builder = match provider.build_chat_request(
            &self.client,
            &target.base_url,
            &target.api_key,
            request,
            &target.upstream_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                return Err(self
                    .finish_with_error(
                        &self.tracking,
                        tracked_request.as_ref(),
                        tracked_execution.as_ref(),
                        Some(&target.upstream_model),
                        None,
                        OpenAiErrorResponse::internal_with(
                            "failed to build upstream chat request",
                            error,
                        ),
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        Ok(PreparedChatRelay {
            request_id,
            started_at,
            tracked_request,
            tracked_execution,
            target,
            provider,
            request_builder,
        })
    }

    async fn send_upstream_chat(
        &self,
        request_builder: reqwest::RequestBuilder,
        provider_kind: ProviderKind,
    ) -> Result<UpstreamChatResponse, OpenAiErrorResponse> {
        let response = request_builder.send().await.map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to call upstream provider", error)
        })?;

        let status = response.status();
        let headers = response.headers().clone();
        let upstream_request_id = extract_upstream_request_id(&headers);
        let body = response.bytes().await.map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to read upstream response", error)
        })?;

        if status.is_success() {
            Ok(UpstreamChatResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: None,
            })
        } else {
            let info =
                ProviderRegistry::get(provider_kind).parse_error(status.as_u16(), &headers, &body);
            Ok(UpstreamChatResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: Some(provider_error_to_openai_response(status.as_u16(), &info)),
            })
        }
    }

    async fn send_upstream_chat_stream(
        &self,
        request_builder: reqwest::RequestBuilder,
        provider_kind: ProviderKind,
    ) -> Result<UpstreamChatStreamResponse, OpenAiErrorResponse> {
        let response = request_builder.send().await.map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to call upstream provider", error)
        })?;

        let status = response.status();
        let headers = response.headers().clone();
        let upstream_request_id = extract_upstream_request_id(&headers);

        if status.is_success() {
            Ok(UpstreamChatStreamResponse::Success {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                response,
            })
        } else {
            let body = response.bytes().await.map_err(|error| {
                OpenAiErrorResponse::internal_with("failed to read upstream response", error)
            })?;
            let info =
                ProviderRegistry::get(provider_kind).parse_error(status.as_u16(), &headers, &body);
            Ok(UpstreamChatStreamResponse::Failure {
                upstream_request_id,
                error: provider_error_to_openai_response(status.as_u16(), &info),
            })
        }
    }

    async fn finish_with_error(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        tracked_execution: Option<&request_execution::Model>,
        upstream_model: Option<&str>,
        upstream_request_id: Option<&str>,
        openai_error: OpenAiErrorResponse,
        duration_ms: i32,
    ) -> OpenAiErrorResponse {
        let error_body =
            serde_json::to_value(&openai_error.error).unwrap_or_else(|_| serde_json::json!({}));
        self.try_finish_request_failure(
            tracking,
            tracked_request,
            upstream_model,
            &openai_error,
            Some(error_body.clone()),
            duration_ms,
        )
        .await;
        self.try_finish_execution_failure(
            tracking,
            tracked_execution,
            upstream_request_id,
            &openai_error,
            Some(error_body),
            duration_ms,
        )
        .await;
        openai_error
    }

    async fn resolve_target(
        &self,
        channel_group: &str,
        request: &ChatCompletionRequest,
    ) -> ApiResult<ResolvedChatTarget> {
        let abilities = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(channel_group))
            .filter(ability::Column::EndpointScope.eq("chat"))
            .filter(ability::Column::Model.eq(request.model.clone()))
            .filter(ability::Column::Enabled.eq(true))
            .order_by_desc(ability::Column::Priority)
            .order_by_desc(ability::Column::Weight)
            .order_by_desc(ability::Column::ChannelId)
            .all(&self.db)
            .await
            .context("查询模型能力失败")?;

        if abilities.is_empty() {
            return Err(ApiErrors::NotFound(format!(
                "model '{}' is not available",
                request.model
            )));
        }

        for ability in abilities {
            let Some(channel) = channel::Entity::find_by_id(ability.channel_id)
                .filter(channel::Column::DeletedAt.is_null())
                .one(&self.db)
                .await
                .context("查询渠道失败")?
            else {
                continue;
            };

            if channel.status != ChannelStatus::Enabled {
                continue;
            }

            let accounts = channel_account::Entity::find()
                .filter(channel_account::Column::ChannelId.eq(channel.id))
                .filter(channel_account::Column::DeletedAt.is_null())
                .order_by_desc(channel_account::Column::Priority)
                .order_by_desc(channel_account::Column::Weight)
                .order_by_desc(channel_account::Column::Id)
                .all(&self.db)
                .await
                .context("查询渠道账号失败")?;

            let Some(account) = select_schedulable_account(&accounts) else {
                continue;
            };

            let Some(api_key) = extract_api_key(&account) else {
                continue;
            };

            let provider_kind = provider_kind_from_channel_type(channel.channel_type)
                .ok_or_else(|| ApiErrors::BadRequest("unsupported channel type".to_string()))?;
            let upstream_model = resolve_upstream_model(&channel, &request.model);
            let base_url = effective_base_url(&channel, provider_kind);

            return Ok(ResolvedChatTarget {
                channel,
                account,
                provider_kind,
                base_url,
                upstream_model,
                api_key,
            });
        }

        Err(ApiErrors::ServiceUnavailable(format!(
            "no available channel for model '{}'",
            request.model
        )))
    }

    async fn try_finish_request_success(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        upstream_model: &str,
        response_status_code: i32,
        response_body: &ChatCompletionResponse,
        duration_ms: i32,
    ) {
        if let Some(tracked_request) = tracked_request {
            if let Err(error) = tracking
                .finish_request_success(
                    tracked_request.id,
                    upstream_model,
                    response_status_code,
                    response_body,
                    duration_ms,
                )
                .await
            {
                tracing::warn!(request_id = tracked_request.request_id, error = %error, "failed to update request success tracking row");
            }
        }
    }

    async fn try_finish_request_failure(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        upstream_model: Option<&str>,
        openai_error: &OpenAiErrorResponse,
        response_body: Option<serde_json::Value>,
        duration_ms: i32,
    ) {
        if let Some(tracked_request) = tracked_request {
            if let Err(error) = tracking
                .finish_request_failure(
                    tracked_request.id,
                    upstream_model,
                    openai_error.status as i32,
                    &openai_error.error.error.message,
                    response_body,
                    duration_ms,
                )
                .await
            {
                tracing::warn!(request_id = tracked_request.request_id, error = %error, "failed to update request failure tracking row");
            }
        }
    }

    async fn try_finish_execution_success(
        &self,
        tracking: &TrackingService,
        tracked_execution: Option<&request_execution::Model>,
        upstream_request_id: Option<&str>,
        response_status_code: i32,
        response_body: &ChatCompletionResponse,
        duration_ms: i32,
    ) {
        if let Some(tracked_execution) = tracked_execution {
            if let Err(error) = tracking
                .finish_execution_success(
                    tracked_execution.id,
                    upstream_request_id,
                    response_status_code,
                    response_body,
                    duration_ms,
                )
                .await
            {
                tracing::warn!(execution_id = tracked_execution.id, error = %error, "failed to update request_execution success tracking row");
            }
        }
    }

    async fn try_finish_execution_failure(
        &self,
        tracking: &TrackingService,
        tracked_execution: Option<&request_execution::Model>,
        upstream_request_id: Option<&str>,
        openai_error: &OpenAiErrorResponse,
        response_body: Option<serde_json::Value>,
        duration_ms: i32,
    ) {
        if let Some(tracked_execution) = tracked_execution {
            if let Err(error) = tracking
                .finish_execution_failure(
                    tracked_execution.id,
                    upstream_request_id,
                    openai_error.status as i32,
                    &openai_error.error.error.message,
                    response_body,
                    duration_ms,
                )
                .await
            {
                tracing::warn!(execution_id = tracked_execution.id, error = %error, "failed to update request_execution failure tracking row");
            }
        }
    }
}

pub(crate) fn provider_kind_from_channel_type(channel_type: ChannelType) -> Option<ProviderKind> {
    ProviderKind::from_channel_type(channel_type as i16)
}

pub(crate) fn effective_base_url(channel: &channel::Model, provider_kind: ProviderKind) -> String {
    if channel.base_url.trim().is_empty() {
        ProviderRegistry::meta(provider_kind)
            .default_base_url
            .to_string()
    } else {
        channel.base_url.clone()
    }
}

pub(crate) fn extract_upstream_request_id(headers: &HeaderMap) -> Option<String> {
    ["x-request-id", "request-id", "anthropic-request-id"]
        .into_iter()
        .find_map(|name| headers.get(name))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn provider_error_to_openai_response(
    status: u16,
    info: &ProviderErrorInfo,
) -> OpenAiErrorResponse {
    let error_type = match info.kind {
        ProviderErrorKind::InvalidRequest => "invalid_request_error",
        ProviderErrorKind::Authentication => "authentication_error",
        ProviderErrorKind::RateLimit => "rate_limit_error",
        ProviderErrorKind::Server => "server_error",
        ProviderErrorKind::Api => "api_error",
    };

    let normalized_status = match info.kind {
        ProviderErrorKind::InvalidRequest => match status {
            404 => 404,
            400 | 413 | 422 => status,
            _ => 400,
        },
        ProviderErrorKind::Authentication => match status {
            403 => 403,
            _ => 401,
        },
        ProviderErrorKind::RateLimit => 429,
        ProviderErrorKind::Server => {
            if (500..=599).contains(&status) {
                status
            } else {
                502
            }
        }
        ProviderErrorKind::Api => {
            if status == 0 || (200..300).contains(&status) {
                502
            } else {
                status
            }
        }
    };

    OpenAiErrorResponse {
        status: normalized_status,
        error: summer_ai_core::types::error::OpenAiError {
            error: summer_ai_core::types::error::OpenAiErrorBody {
                message: info.message.clone(),
                r#type: error_type.to_string(),
                param: None,
                code: Some(info.code.to_lowercase()),
            },
        },
    }
}

pub fn extract_api_key(account: &channel_account::Model) -> Option<String> {
    account
        .credentials
        .get("api_key")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn resolve_upstream_model(channel: &channel::Model, requested_model: &str) -> String {
    channel
        .model_mapping
        .get(requested_model)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(requested_model)
        .to_string()
}

pub fn select_schedulable_account(
    accounts: &[channel_account::Model],
) -> Option<channel_account::Model> {
    accounts
        .iter()
        .filter(|account| account.deleted_at.is_none())
        .filter(|account| account.schedulable)
        .filter(|account| account.status == ChannelAccountStatus::Enabled)
        .max_by_key(|account| (account.priority, account.weight, account.id))
        .cloned()
}

#[derive(Clone)]
pub struct RelayChatContext {
    pub token_info: TokenInfo,
    pub client_ip: String,
    pub user_agent: String,
    pub request_headers: HeaderMap,
}

#[derive(Clone)]
struct ResolvedChatTarget {
    channel: channel::Model,
    account: channel_account::Model,
    provider_kind: ProviderKind,
    base_url: String,
    upstream_model: String,
    api_key: String,
}

struct PreparedChatRelay {
    request_id: String,
    started_at: Instant,
    tracked_request: Option<request::Model>,
    tracked_execution: Option<request_execution::Model>,
    target: ResolvedChatTarget,
    provider: &'static dyn ChatProvider,
    request_builder: reqwest::RequestBuilder,
}

enum UpstreamChatStreamResponse {
    Success {
        status_code: i32,
        upstream_request_id: Option<String>,
        response: reqwest::Response,
    },
    Failure {
        upstream_request_id: Option<String>,
        error: OpenAiErrorResponse,
    },
}

struct UpstreamChatResponse {
    status_code: i32,
    upstream_request_id: Option<String>,
    body: bytes::Bytes,
    error: Option<OpenAiErrorResponse>,
}

fn stream_error_status_code(error: &anyhow::Error) -> i32 {
    error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .map(|error| match error.info.kind {
            summer_ai_core::provider::ProviderErrorKind::InvalidRequest => 400,
            summer_ai_core::provider::ProviderErrorKind::Authentication => 401,
            summer_ai_core::provider::ProviderErrorKind::RateLimit => 429,
            summer_ai_core::provider::ProviderErrorKind::Server
            | summer_ai_core::provider::ProviderErrorKind::Api => 502,
        })
        .unwrap_or(0)
}

fn stream_error_message(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .map(|error| error.info.message.clone())
        .unwrap_or_else(|| error.to_string())
}

fn build_tracking_upstream_body(
    request: &ChatCompletionRequest,
    upstream_model: &str,
) -> serde_json::Value {
    let mut body = serde_json::to_value(request).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(object) = body.as_object_mut() {
        object.insert(
            "model".to_string(),
            serde_json::Value::String(upstream_model.to_string()),
        );
    }
    body
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use summer_ai_model::entity::channel::{
        self, ChannelLastHealthStatus, ChannelStatus, ChannelType,
    };
    use summer_ai_model::entity::channel_account::{self, ChannelAccountStatus};
    use summer_common::error::ApiResult;
    use summer_web::axum::http::HeaderMap;

    use super::{
        RelayChatContext, extract_api_key, resolve_upstream_model, select_schedulable_account,
    };
    use crate::service::token::TokenInfo;

    fn sample_channel() -> channel::Model {
        let now = Utc::now().fixed_offset();
        channel::Model {
            id: 1,
            name: "openai-primary".into(),
            channel_type: ChannelType::OpenAi,
            vendor_code: "openai".into(),
            base_url: "https://api.openai.com".into(),
            status: ChannelStatus::Enabled,
            models: serde_json::json!(["gpt-4o"]),
            model_mapping: serde_json::json!({"gpt-4o": "gpt-4o-2026-01-01"}),
            channel_group: "default".into(),
            endpoint_scopes: serde_json::json!(["chat"]),
            capabilities: serde_json::json!(["streaming"]),
            weight: 1,
            priority: 10,
            config: serde_json::json!({}),
            auto_ban: true,
            test_model: "gpt-4o".into(),
            used_quota: 0,
            balance: 0.into(),
            balance_updated_at: None,
            response_time: 0,
            success_rate: 0.into(),
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            last_health_status: ChannelLastHealthStatus::Unknown,
            deleted_at: None,
            remark: String::new(),
            create_by: "system".into(),
            create_time: now,
            update_by: "system".into(),
            update_time: now,
        }
    }

    fn sample_account(
        id: i64,
        priority: i32,
        schedulable: bool,
        status: ChannelAccountStatus,
    ) -> channel_account::Model {
        let now = Utc::now().fixed_offset();
        channel_account::Model {
            id,
            channel_id: 1,
            name: format!("account-{id}"),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({"api_key": format!("sk-{id}")}),
            secret_ref: String::new(),
            status,
            schedulable,
            priority,
            weight: 1,
            rate_multiplier: 1.into(),
            concurrency_limit: 0,
            quota_limit: 0.into(),
            quota_used: 0.into(),
            balance: 0.into(),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            rate_limited_until: None,
            overload_until: None,
            expires_at: None,
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "system".into(),
            create_time: now,
            update_by: "system".into(),
            update_time: now,
        }
    }

    #[test]
    fn extract_api_key_reads_api_key_from_credentials() {
        let account = sample_account(1, 1, true, ChannelAccountStatus::Enabled);
        assert_eq!(extract_api_key(&account).as_deref(), Some("sk-1"));
    }

    #[test]
    fn resolve_upstream_model_prefers_channel_mapping() {
        let channel = sample_channel();
        assert_eq!(
            resolve_upstream_model(&channel, "gpt-4o"),
            "gpt-4o-2026-01-01"
        );
        assert_eq!(resolve_upstream_model(&channel, "gpt-4.1"), "gpt-4.1");
    }

    #[test]
    fn select_schedulable_account_prefers_enabled_schedulable_high_priority_account() {
        let disabled = sample_account(1, 100, true, ChannelAccountStatus::Disabled);
        let low = sample_account(2, 10, true, ChannelAccountStatus::Enabled);
        let high = sample_account(3, 20, true, ChannelAccountStatus::Enabled);

        let selected = select_schedulable_account(&[disabled, low, high]).expect("select account");
        assert_eq!(selected.id, 3);
    }

    #[test]
    fn relay_chat_context_keeps_request_metadata_together() -> ApiResult<()> {
        let ctx = RelayChatContext {
            token_info: TokenInfo {
                token_id: 1,
                user_id: 2,
                name: "demo".into(),
                group: "default".into(),
                remain_quota: 100,
                unlimited_quota: false,
                rpm_limit: 0,
                tpm_limit: 0,
                concurrency_limit: 0,
                allowed_models: vec![],
                endpoint_scopes: vec![],
            },
            client_ip: "127.0.0.1".into(),
            user_agent: "codex-test".into(),
            request_headers: HeaderMap::new(),
        };

        assert_eq!(ctx.client_ip, "127.0.0.1");
        assert_eq!(ctx.user_agent, "codex-test");
        Ok(())
    }
}
