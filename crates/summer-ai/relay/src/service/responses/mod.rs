use anyhow::Context;
use bytes::Bytes;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_core::provider::{ProviderKind, ProviderRegistry, ResponsesRuntimeMode};
use summer_ai_core::types::chat::ChatCompletionResponse;
use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_ai_core::types::responses::{
    ResponseInputTokensDetails, ResponseOutputTokensDetails, ResponseUsage, ResponsesRequest,
    ResponsesResponse,
};
use summer_common::response::Json;
use summer_sea_orm::DbConn;
use summer_web::axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::service::chat::{
    RelayChatContext, effective_base_url, extract_api_key, extract_upstream_request_id,
    provider_error_to_openai_response, provider_kind_from_channel_type, resolve_upstream_model,
    select_schedulable_account,
};
use crate::service::tracking::TrackingService;
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel::ChannelStatus;
use summer_ai_model::entity::channel_account;
use summer_ai_model::entity::request;
use summer_ai_model::entity::request_execution;
use summer_common::error::{ApiErrors, ApiResult};

#[derive(Clone, Service)]
pub struct ResponsesRelayService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    client: reqwest::Client,
    #[inject(component)]
    tracking: TrackingService,
}

impl ResponsesRelayService {
    pub async fn relay(
        &self,
        ctx: RelayChatContext,
        request: ResponsesRequest,
    ) -> Result<Response, OpenAiErrorResponse> {
        ctx.token_info
            .ensure_endpoint_allowed("responses")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        ctx.token_info
            .ensure_model_allowed(&request.model)
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;

        let request_id = format!("req_{}", Uuid::new_v4().simple());
        let started_at = std::time::Instant::now();
        let tracking = &self.tracking;

        let tracked_request = match tracking
            .create_responses_request(
                &request_id,
                &ctx.token_info,
                &request,
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

        if request.stream {
            let error = OpenAiErrorResponse::invalid_request(
                "streaming responses relay is not implemented yet",
            );
            return Err(self
                .finish_with_error(
                    tracking,
                    tracked_request.as_ref(),
                    None,
                    None,
                    None,
                    error,
                    started_at.elapsed().as_millis() as i32,
                )
                .await);
        }

        let target = match self.resolve_target(&ctx.token_info.group, &request).await {
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
                        tracking,
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
            let upstream_body = build_tracking_upstream_body(&request, &target.upstream_model);
            match tracking
                .create_responses_execution(
                    tracked_request.id,
                    &request_id,
                    &request,
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

        let provider = ProviderRegistry::responses(target.provider_kind).ok_or_else(|| {
            OpenAiErrorResponse::unsupported_endpoint("responses endpoint is disabled")
        })?;

        let request_builder = match provider.build_responses_request(
            &self.client,
            &target.base_url,
            &target.api_key,
            &serde_json::to_value(&request).unwrap_or_else(|_| serde_json::json!({})),
            &target.upstream_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                return Err(self
                    .finish_with_error(
                        tracking,
                        tracked_request.as_ref(),
                        tracked_execution.as_ref(),
                        Some(&target.upstream_model),
                        None,
                        OpenAiErrorResponse::internal_with(
                            "failed to build upstream responses request",
                            error,
                        ),
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        let upstream_response = match self
            .send_upstream_responses(request_builder, target.provider_kind)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return Err(self
                    .finish_with_error(
                        tracking,
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
                    tracking,
                    tracked_request.as_ref(),
                    tracked_execution.as_ref(),
                    Some(&target.upstream_model),
                    upstream_response.upstream_request_id.as_deref(),
                    error,
                    started_at.elapsed().as_millis() as i32,
                )
                .await);
        }

        let responses_response = match self.parse_responses_response(
            provider.runtime_mode(),
            target.provider_kind,
            upstream_response.body,
            &target.upstream_model,
        ) {
            Ok(response) => response,
            Err(error) => {
                return Err(self
                    .finish_with_error(
                        tracking,
                        tracked_request.as_ref(),
                        tracked_execution.as_ref(),
                        Some(&target.upstream_model),
                        upstream_response.upstream_request_id.as_deref(),
                        OpenAiErrorResponse::internal_with(
                            "failed to parse upstream responses response",
                            error,
                        ),
                        started_at.elapsed().as_millis() as i32,
                    )
                    .await);
            }
        };

        let duration_ms = started_at.elapsed().as_millis() as i32;
        self.try_finish_request_success(
            tracking,
            tracked_request.as_ref(),
            &target.upstream_model,
            upstream_response.status_code,
            &responses_response,
            duration_ms,
        )
        .await;
        self.try_finish_execution_success(
            tracking,
            tracked_execution.as_ref(),
            upstream_response.upstream_request_id.as_deref(),
            upstream_response.status_code,
            &responses_response,
            duration_ms,
        )
        .await;

        Ok(Json::<ResponsesResponse>(responses_response).into_response())
    }

    async fn send_upstream_responses(
        &self,
        request_builder: reqwest::RequestBuilder,
        provider_kind: ProviderKind,
    ) -> Result<UpstreamResponsesResponse, OpenAiErrorResponse> {
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
            Ok(UpstreamResponsesResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: None,
            })
        } else {
            let info =
                ProviderRegistry::get(provider_kind).parse_error(status.as_u16(), &headers, &body);
            Ok(UpstreamResponsesResponse {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                body,
                error: Some(provider_error_to_openai_response(status.as_u16(), &info)),
            })
        }
    }

    fn parse_responses_response(
        &self,
        runtime_mode: ResponsesRuntimeMode,
        provider_kind: ProviderKind,
        body: Bytes,
        upstream_model: &str,
    ) -> anyhow::Result<ResponsesResponse> {
        match runtime_mode {
            ResponsesRuntimeMode::Native => serde_json::from_slice(&body).map_err(Into::into),
            ResponsesRuntimeMode::ChatBridge => {
                let provider = ProviderRegistry::chat(provider_kind)
                    .ok_or_else(|| anyhow::anyhow!("responses bridge requires chat provider"))?;
                let response = provider
                    .parse_chat_response(body, upstream_model)
                    .context("failed to parse bridged chat response")?;
                Ok(bridge_chat_response_to_responses_response(&response))
            }
        }
    }

    async fn resolve_target(
        &self,
        channel_group: &str,
        request: &ResponsesRequest,
    ) -> ApiResult<ResolvedResponsesTarget> {
        let abilities = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(channel_group))
            .filter(ability::Column::EndpointScope.eq("responses"))
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

            return Ok(ResolvedResponsesTarget {
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

    async fn try_finish_request_success(
        &self,
        tracking: &TrackingService,
        tracked_request: Option<&request::Model>,
        upstream_model: &str,
        response_status_code: i32,
        response_body: &ResponsesResponse,
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
        response_body: &ResponsesResponse,
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

fn bridge_chat_response_to_responses_response(
    response: &ChatCompletionResponse,
) -> ResponsesResponse {
    ResponsesResponse {
        id: response.id.clone(),
        object: "response".into(),
        created_at: response.created,
        model: response.model.clone(),
        status: "completed".into(),
        usage: Some(ResponseUsage {
            input_tokens: response.usage.prompt_tokens,
            output_tokens: response.usage.completion_tokens,
            total_tokens: response.usage.total_tokens,
            input_tokens_details: (response.usage.cached_tokens > 0).then_some(
                ResponseInputTokensDetails {
                    cached_tokens: response.usage.cached_tokens,
                },
            ),
            output_tokens_details: (response.usage.reasoning_tokens > 0).then_some(
                ResponseOutputTokensDetails {
                    reasoning_tokens: response.usage.reasoning_tokens,
                },
            ),
        }),
        output_text: response
            .choices
            .first()
            .and_then(|choice| choice.message.text_content())
            .map(ToOwned::to_owned),
        extra: serde_json::Map::new(),
    }
}

#[derive(Clone)]
struct ResolvedResponsesTarget {
    channel: channel::Model,
    account: channel_account::Model,
    provider_kind: ProviderKind,
    base_url: String,
    upstream_model: String,
    api_key: String,
}

struct UpstreamResponsesResponse {
    status_code: i32,
    upstream_request_id: Option<String>,
    body: Bytes,
    error: Option<OpenAiErrorResponse>,
}

fn build_tracking_upstream_body(
    request: &ResponsesRequest,
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
    use summer_ai_core::types::chat::ChatCompletionResponse;

    use super::bridge_chat_response_to_responses_response;

    #[test]
    fn bridge_chat_response_to_responses_response_preserves_usage_and_text() {
        let chat: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
            "id": "chatcmpl_123",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "gpt-5.4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "hello from chat"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 11,
                "completion_tokens": 7,
                "total_tokens": 18,
                "cached_tokens": 2,
                "reasoning_tokens": 3
            }
        }))
        .expect("chat response");

        let bridged = bridge_chat_response_to_responses_response(&chat);
        assert_eq!(bridged.object, "response");
        assert_eq!(bridged.model, "gpt-5.4");
        assert_eq!(bridged.output_text.as_deref(), Some("hello from chat"));
        assert_eq!(bridged.status, "completed");

        let usage = bridged.usage.expect("usage");
        assert_eq!(usage.input_tokens, 11);
        assert_eq!(usage.output_tokens, 7);
        assert_eq!(usage.total_tokens, 18);
        assert_eq!(
            usage
                .input_tokens_details
                .expect("input details")
                .cached_tokens,
            2
        );
        assert_eq!(
            usage
                .output_tokens_details
                .expect("output details")
                .reasoning_tokens,
            3
        );
    }
}
