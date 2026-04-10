use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Serialize;
use summer::plugin::Service;
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::embedding::EmbeddingRequest;
use summer_ai_core::types::responses::ResponsesRequest;
use summer_ai_model::entity::request::{self, RequestStatus};
use summer_ai_model::entity::request_execution::{self, RequestExecutionStatus};
use summer_ai_model::entity::retry_attempt::{self, RetryAttemptStatus};
use summer_ai_model::entity::trace::{self, TraceStatus};
use summer_ai_model::entity::trace_span::{self, TraceSpanStatus};
use summer_common::error::ApiResult;
use summer_sea_orm::DbConn;
use summer_web::axum::http::HeaderMap;

use crate::service::token::TokenInfo;

pub const CHAT_COMPLETIONS_ENDPOINT: &str = "/v1/chat/completions";
pub const CHAT_COMPLETIONS_FORMAT: &str = "openai/chat_completions";
pub const EMBEDDINGS_ENDPOINT: &str = "/v1/embeddings";
pub const EMBEDDINGS_FORMAT: &str = "openai/embeddings";
pub const RESPONSES_ENDPOINT: &str = "/v1/responses";
pub const RESPONSES_FORMAT: &str = "openai/responses";

pub(crate) fn execution_trace_span_key(attempt_no: i32) -> String {
    format!("execution:{}", attempt_no.max(1))
}

pub(crate) fn build_request_trace_success_metadata(
    request_id: &str,
    endpoint: &str,
    request_format: &str,
    requested_model: &str,
    upstream_model: &str,
    is_stream: bool,
    response_status_code: i32,
    duration_ms: i32,
    first_token_ms: i32,
) -> serde_json::Value {
    serde_json::json!({
        "request_id": request_id,
        "endpoint": endpoint,
        "request_format": request_format,
        "requested_model": requested_model,
        "upstream_model": upstream_model,
        "is_stream": is_stream,
        "response_status_code": response_status_code,
        "duration_ms": duration_ms,
        "first_token_ms": first_token_ms,
        "status": "succeeded",
    })
}

pub(crate) fn request_trace_success_metadata(
    tracked_request: &request::Model,
    upstream_model: &str,
    response_status_code: i32,
    duration_ms: i32,
    first_token_ms: i32,
) -> serde_json::Value {
    build_request_trace_success_metadata(
        &tracked_request.request_id,
        &tracked_request.endpoint,
        &tracked_request.request_format,
        &tracked_request.requested_model,
        upstream_model,
        tracked_request.is_stream,
        response_status_code,
        duration_ms,
        first_token_ms,
    )
}

pub(crate) fn build_request_trace_failure_metadata(
    request_id: &str,
    endpoint: &str,
    request_format: &str,
    requested_model: &str,
    upstream_model: Option<&str>,
    is_stream: bool,
    response_status_code: i32,
    error_message: &str,
    duration_ms: i32,
    first_token_ms: i32,
) -> serde_json::Value {
    serde_json::json!({
        "request_id": request_id,
        "endpoint": endpoint,
        "request_format": request_format,
        "requested_model": requested_model,
        "upstream_model": upstream_model.unwrap_or_default(),
        "is_stream": is_stream,
        "response_status_code": response_status_code,
        "error_message": error_message,
        "duration_ms": duration_ms,
        "first_token_ms": first_token_ms,
        "status": "failed",
    })
}

pub(crate) fn request_trace_failure_metadata(
    tracked_request: &request::Model,
    upstream_model: Option<&str>,
    response_status_code: i32,
    error_message: &str,
    duration_ms: i32,
    first_token_ms: i32,
) -> serde_json::Value {
    build_request_trace_failure_metadata(
        &tracked_request.request_id,
        &tracked_request.endpoint,
        &tracked_request.request_format,
        &tracked_request.requested_model,
        upstream_model.or_else(|| Some(tracked_request.upstream_model.as_str())),
        tracked_request.is_stream,
        response_status_code,
        error_message,
        duration_ms,
        first_token_ms,
    )
}

pub(crate) fn execution_trace_span_success_metadata(
    tracked_execution: &request_execution::Model,
    upstream_request_id: Option<&str>,
    response_status_code: i32,
    duration_ms: i32,
    first_token_ms: i32,
) -> serde_json::Value {
    serde_json::json!({
        "request_id": tracked_execution.request_id,
        "execution_id": tracked_execution.id,
        "attempt_no": tracked_execution.attempt_no,
        "response_status_code": response_status_code,
        "upstream_request_id": upstream_request_id.unwrap_or(tracked_execution.upstream_request_id.as_str()),
        "duration_ms": duration_ms,
        "first_token_ms": first_token_ms,
        "status": "succeeded",
    })
}

pub(crate) fn execution_trace_span_failure_metadata(
    tracked_execution: &request_execution::Model,
    upstream_request_id: Option<&str>,
    response_status_code: i32,
    error_message: &str,
    duration_ms: i32,
    first_token_ms: i32,
) -> serde_json::Value {
    serde_json::json!({
        "request_id": tracked_execution.request_id,
        "execution_id": tracked_execution.id,
        "attempt_no": tracked_execution.attempt_no,
        "response_status_code": response_status_code,
        "upstream_request_id": upstream_request_id.unwrap_or(tracked_execution.upstream_request_id.as_str()),
        "error_message": error_message,
        "duration_ms": duration_ms,
        "first_token_ms": first_token_ms,
        "status": "failed",
    })
}

#[derive(Clone, Service)]
pub struct TrackingService {
    #[inject(component)]
    db: DbConn,
}

impl TrackingService {
    pub async fn create_trace(&self, input: CreateTraceTracking<'_>) -> ApiResult<trace::Model> {
        input
            .into_active_model()
            .insert(&self.db)
            .await
            .context("创建 trace 追踪记录失败")
            .map_err(Into::into)
    }

    pub async fn finish_trace_success(
        &self,
        trace_id: i64,
        metadata: serde_json::Value,
    ) -> ApiResult<()> {
        self.finish_trace(trace_id, TraceStatus::Succeeded, metadata)
            .await
    }

    pub async fn finish_trace_failure(
        &self,
        trace_id: i64,
        metadata: serde_json::Value,
    ) -> ApiResult<()> {
        self.finish_trace(trace_id, TraceStatus::Failed, metadata)
            .await
    }

    pub async fn create_trace_span(
        &self,
        input: CreateTraceSpanTracking<'_>,
    ) -> ApiResult<trace_span::Model> {
        input
            .into_active_model()
            .insert(&self.db)
            .await
            .context("创建 trace_span 追踪记录失败")
            .map_err(Into::into)
    }

    pub async fn finish_trace_span_success(
        &self,
        trace_id: i64,
        span_key: &str,
        output_payload: serde_json::Value,
        metadata: serde_json::Value,
    ) -> ApiResult<()> {
        self.finish_trace_span(
            trace_id,
            span_key,
            TraceSpanStatus::Succeeded,
            output_payload,
            "",
            metadata,
        )
        .await
    }

    pub async fn finish_trace_span_failure(
        &self,
        trace_id: i64,
        span_key: &str,
        error_message: &str,
        output_payload: serde_json::Value,
        metadata: serde_json::Value,
    ) -> ApiResult<()> {
        self.finish_trace_span(
            trace_id,
            span_key,
            TraceSpanStatus::Failed,
            output_payload,
            error_message,
            metadata,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_execution_trace_span(
        &self,
        trace_id: i64,
        request_id: &str,
        endpoint: &str,
        attempt_no: i32,
        requested_model: &str,
        upstream_model: &str,
        channel_id: i64,
        account_id: i64,
        input_payload: serde_json::Value,
    ) -> ApiResult<trace_span::Model> {
        let span_key = execution_trace_span_key(attempt_no);
        let span_name = format!("{endpoint} upstream attempt #{attempt_no}");
        let target_ref = format!("{channel_id}:{account_id}");

        self.create_trace_span(CreateTraceSpanTracking {
            trace_id,
            parent_span_id: 0,
            span_key: &span_key,
            span_name: &span_name,
            span_type: "llm",
            target_kind: "channel_account",
            target_ref: &target_ref,
            input_payload,
            metadata: serde_json::json!({
                "request_id": request_id,
                "endpoint": endpoint,
                "attempt_no": attempt_no.max(1),
                "requested_model": requested_model,
                "upstream_model": upstream_model,
                "channel_id": channel_id,
                "account_id": account_id,
            }),
        })
        .await
    }

    pub async fn finish_execution_trace_span_success(
        &self,
        trace_id: i64,
        attempt_no: i32,
        output_payload: serde_json::Value,
        metadata: serde_json::Value,
    ) -> ApiResult<()> {
        self.finish_trace_span_success(
            trace_id,
            &execution_trace_span_key(attempt_no),
            output_payload,
            metadata,
        )
        .await
    }

    pub async fn finish_execution_trace_span_failure(
        &self,
        trace_id: i64,
        attempt_no: i32,
        error_message: &str,
        output_payload: serde_json::Value,
        metadata: serde_json::Value,
    ) -> ApiResult<()> {
        self.finish_trace_span_failure(
            trace_id,
            &execution_trace_span_key(attempt_no),
            error_message,
            output_payload,
            metadata,
        )
        .await
    }

    pub async fn create_chat_request(
        &self,
        request_id: &str,
        trace_id: i64,
        token_info: &TokenInfo,
        request: &ChatCompletionRequest,
        client_ip: &str,
        user_agent: &str,
        headers: &HeaderMap,
    ) -> ApiResult<request::Model> {
        CreateChatRequestTracking {
            request_id,
            trace_id,
            token_info,
            request,
            client_ip,
            user_agent,
            headers,
        }
        .into_active_model()
        .insert(&self.db)
        .await
        .context("创建 request 追踪记录失败")
        .map_err(Into::into)
    }

    pub async fn create_chat_execution(
        &self,
        ai_request_id: i64,
        request_id: &str,
        attempt_no: i32,
        request: &ChatCompletionRequest,
        channel_id: i64,
        account_id: i64,
        upstream_model: &str,
        request_body: serde_json::Value,
    ) -> ApiResult<request_execution::Model> {
        CreateChatExecutionTracking {
            ai_request_id,
            request_id,
            attempt_no,
            request,
            channel_id,
            account_id,
            upstream_model,
            request_body,
        }
        .into_active_model()
        .insert(&self.db)
        .await
        .context("创建 request_execution 追踪记录失败")
        .map_err(Into::into)
    }

    pub async fn create_responses_request(
        &self,
        request_id: &str,
        trace_id: i64,
        token_info: &TokenInfo,
        request: &ResponsesRequest,
        client_ip: &str,
        user_agent: &str,
        headers: &HeaderMap,
    ) -> ApiResult<request::Model> {
        CreateResponsesRequestTracking {
            request_id,
            trace_id,
            token_info,
            request,
            client_ip,
            user_agent,
            headers,
        }
        .into_active_model()
        .insert(&self.db)
        .await
        .context("创建 request 追踪记录失败")
        .map_err(Into::into)
    }

    pub async fn create_responses_execution(
        &self,
        ai_request_id: i64,
        request_id: &str,
        attempt_no: i32,
        request: &ResponsesRequest,
        channel_id: i64,
        account_id: i64,
        upstream_model: &str,
        request_body: serde_json::Value,
    ) -> ApiResult<request_execution::Model> {
        CreateResponsesExecutionTracking {
            ai_request_id,
            request_id,
            attempt_no,
            request,
            channel_id,
            account_id,
            upstream_model,
            request_body,
        }
        .into_active_model()
        .insert(&self.db)
        .await
        .context("创建 request_execution 追踪记录失败")
        .map_err(Into::into)
    }

    pub async fn create_embeddings_request(
        &self,
        request_id: &str,
        trace_id: i64,
        token_info: &TokenInfo,
        request: &EmbeddingRequest,
        client_ip: &str,
        user_agent: &str,
        headers: &HeaderMap,
    ) -> ApiResult<request::Model> {
        CreateEmbeddingsRequestTracking {
            request_id,
            trace_id,
            token_info,
            request,
            client_ip,
            user_agent,
            headers,
        }
        .into_active_model()
        .insert(&self.db)
        .await
        .context("创建 request 追踪记录失败")
        .map_err(Into::into)
    }

    pub async fn create_embeddings_execution(
        &self,
        ai_request_id: i64,
        request_id: &str,
        attempt_no: i32,
        request: &EmbeddingRequest,
        channel_id: i64,
        account_id: i64,
        upstream_model: &str,
        request_body: serde_json::Value,
    ) -> ApiResult<request_execution::Model> {
        CreateEmbeddingsExecutionTracking {
            ai_request_id,
            request_id,
            attempt_no,
            request,
            channel_id,
            account_id,
            upstream_model,
            request_body,
        }
        .into_active_model()
        .insert(&self.db)
        .await
        .context("创建 request_execution 追踪记录失败")
        .map_err(Into::into)
    }

    pub async fn create_retry_attempt(
        &self,
        input: CreateRetryAttemptTracking<'_>,
    ) -> ApiResult<retry_attempt::Model> {
        input
            .into_active_model()
            .insert(&self.db)
            .await
            .context("创建 retry_attempt 记录失败")
            .map_err(Into::into)
    }

    pub async fn finish_retry_attempt(
        &self,
        retry_attempt_id: i64,
        status: RetryAttemptStatus,
        error_message: &str,
        payload: serde_json::Value,
    ) -> ApiResult<()> {
        let mut active: retry_attempt::ActiveModel =
            retry_attempt::Entity::find_by_id(retry_attempt_id)
                .one(&self.db)
                .await
                .context("查询 retry_attempt 记录失败")?
                .context("retry_attempt 记录不存在")?
                .into();

        active.status = Set(status);
        active.error_message = Set(error_message.to_string());
        active.payload = Set(payload);
        active
            .update(&self.db)
            .await
            .context("更新 retry_attempt 记录失败")?;
        Ok(())
    }

    pub async fn finish_request_success<T: Serialize>(
        &self,
        request_pk: i64,
        upstream_model: &str,
        response_status_code: i32,
        response_body: &T,
        duration_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request::ActiveModel = request::Entity::find_by_id(request_pk)
            .one(&self.db)
            .await
            .context("查询 request 追踪记录失败")?
            .context("request 追踪记录不存在")?
            .into();

        active.upstream_model = Set(upstream_model.to_string());
        active.response_status_code = Set(response_status_code);
        active.response_body = Set(Some(
            serde_json::to_value(response_body).unwrap_or_else(|_| serde_json::json!({})),
        ));
        active.status = Set(RequestStatus::Succeeded);
        active.error_message = Set(String::new());
        active.duration_ms = Set(duration_ms);
        active
            .update(&self.db)
            .await
            .context("更新 request 成功记录失败")?;
        Ok(())
    }

    pub async fn record_request_first_token(
        &self,
        request_pk: i64,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request::ActiveModel = request::Entity::find_by_id(request_pk)
            .one(&self.db)
            .await
            .context("查询 request 追踪记录失败")?
            .context("request 追踪记录不存在")?
            .into();

        active.first_token_ms = Set(first_token_ms);
        active
            .update(&self.db)
            .await
            .context("更新 request 首 token 延迟失败")?;
        Ok(())
    }

    pub async fn finish_request_failure(
        &self,
        request_pk: i64,
        upstream_model: Option<&str>,
        response_status_code: i32,
        error_message: &str,
        response_body: Option<serde_json::Value>,
        duration_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request::ActiveModel = request::Entity::find_by_id(request_pk)
            .one(&self.db)
            .await
            .context("查询 request 失败记录失败")?
            .context("request 失败记录不存在")?
            .into();

        if let Some(upstream_model) = upstream_model {
            active.upstream_model = Set(upstream_model.to_string());
        }
        active.response_status_code = Set(response_status_code);
        active.response_body = Set(response_body);
        active.status = Set(RequestStatus::Failed);
        active.error_message = Set(error_message.to_string());
        active.duration_ms = Set(duration_ms);
        active
            .update(&self.db)
            .await
            .context("更新 request 失败记录失败")?;
        Ok(())
    }

    pub async fn finish_request_stream_success(
        &self,
        request_pk: i64,
        upstream_model: &str,
        response_status_code: i32,
        duration_ms: i32,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request::ActiveModel = request::Entity::find_by_id(request_pk)
            .one(&self.db)
            .await
            .context("查询 request 追踪记录失败")?
            .context("request 追踪记录不存在")?
            .into();

        active.upstream_model = Set(upstream_model.to_string());
        active.response_status_code = Set(response_status_code);
        active.response_body = Set(None);
        active.status = Set(RequestStatus::Succeeded);
        active.error_message = Set(String::new());
        active.duration_ms = Set(duration_ms);
        active.first_token_ms = Set(first_token_ms);
        active
            .update(&self.db)
            .await
            .context("更新 request 流式成功记录失败")?;
        Ok(())
    }

    pub async fn finish_request_stream_failure(
        &self,
        request_pk: i64,
        upstream_model: Option<&str>,
        response_status_code: i32,
        error_message: &str,
        response_body: Option<serde_json::Value>,
        duration_ms: i32,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request::ActiveModel = request::Entity::find_by_id(request_pk)
            .one(&self.db)
            .await
            .context("查询 request 失败记录失败")?
            .context("request 失败记录不存在")?
            .into();

        if let Some(upstream_model) = upstream_model {
            active.upstream_model = Set(upstream_model.to_string());
        }
        active.response_status_code = Set(response_status_code);
        active.response_body = Set(response_body);
        active.status = Set(RequestStatus::Failed);
        active.error_message = Set(error_message.to_string());
        active.duration_ms = Set(duration_ms);
        active.first_token_ms = Set(first_token_ms);
        active
            .update(&self.db)
            .await
            .context("更新 request 流式失败记录失败")?;
        Ok(())
    }

    pub async fn finish_request_trace_from_request_success(
        &self,
        request_pk: i64,
        upstream_model: &str,
        response_status_code: i32,
        duration_ms: i32,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let tracked_request = request::Entity::find_by_id(request_pk)
            .one(&self.db)
            .await
            .context("查询 request trace 记录失败")?
            .context("request trace 记录不存在")?;

        if tracked_request.trace_id <= 0 {
            return Ok(());
        }

        self.finish_trace_success(
            tracked_request.trace_id,
            request_trace_success_metadata(
                &tracked_request,
                upstream_model,
                response_status_code,
                duration_ms,
                first_token_ms,
            ),
        )
        .await
    }

    pub async fn finish_request_trace_from_request_failure(
        &self,
        request_pk: i64,
        upstream_model: Option<&str>,
        response_status_code: i32,
        error_message: &str,
        duration_ms: i32,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let tracked_request = request::Entity::find_by_id(request_pk)
            .one(&self.db)
            .await
            .context("查询 request trace 记录失败")?
            .context("request trace 记录不存在")?;

        if tracked_request.trace_id <= 0 {
            return Ok(());
        }

        self.finish_trace_failure(
            tracked_request.trace_id,
            request_trace_failure_metadata(
                &tracked_request,
                upstream_model,
                response_status_code,
                error_message,
                duration_ms,
                first_token_ms,
            ),
        )
        .await
    }

    pub async fn finish_execution_success<T: Serialize>(
        &self,
        execution_id: i64,
        upstream_request_id: Option<&str>,
        response_status_code: i32,
        response_body: &T,
        duration_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request_execution::ActiveModel =
            request_execution::Entity::find_by_id(execution_id)
                .one(&self.db)
                .await
                .context("查询 request_execution 追踪记录失败")?
                .context("request_execution 追踪记录不存在")?
                .into();

        if let Some(upstream_request_id) = upstream_request_id {
            active.upstream_request_id = Set(upstream_request_id.to_string());
        }
        active.response_status_code = Set(response_status_code);
        active.response_body = Set(Some(
            serde_json::to_value(response_body).unwrap_or_else(|_| serde_json::json!({})),
        ));
        active.status = Set(RequestExecutionStatus::Succeeded);
        active.error_message = Set(String::new());
        active.duration_ms = Set(duration_ms);
        active.finished_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active
            .update(&self.db)
            .await
            .context("更新 request_execution 成功记录失败")?;
        Ok(())
    }

    pub async fn record_execution_first_token(
        &self,
        execution_id: i64,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request_execution::ActiveModel =
            request_execution::Entity::find_by_id(execution_id)
                .one(&self.db)
                .await
                .context("查询 request_execution 追踪记录失败")?
                .context("request_execution 追踪记录不存在")?
                .into();

        active.first_token_ms = Set(first_token_ms);
        active
            .update(&self.db)
            .await
            .context("更新 request_execution 首 token 延迟失败")?;
        Ok(())
    }

    pub async fn finish_execution_failure(
        &self,
        execution_id: i64,
        upstream_request_id: Option<&str>,
        response_status_code: i32,
        error_message: &str,
        response_body: Option<serde_json::Value>,
        duration_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request_execution::ActiveModel =
            request_execution::Entity::find_by_id(execution_id)
                .one(&self.db)
                .await
                .context("查询 request_execution 失败记录失败")?
                .context("request_execution 失败记录不存在")?
                .into();

        if let Some(upstream_request_id) = upstream_request_id {
            active.upstream_request_id = Set(upstream_request_id.to_string());
        }
        active.response_status_code = Set(response_status_code);
        active.response_body = Set(response_body);
        active.status = Set(RequestExecutionStatus::Failed);
        active.error_message = Set(error_message.to_string());
        active.duration_ms = Set(duration_ms);
        active.finished_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active
            .update(&self.db)
            .await
            .context("更新 request_execution 失败记录失败")?;
        Ok(())
    }

    pub async fn finish_execution_stream_success(
        &self,
        execution_id: i64,
        upstream_request_id: Option<&str>,
        response_status_code: i32,
        duration_ms: i32,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request_execution::ActiveModel =
            request_execution::Entity::find_by_id(execution_id)
                .one(&self.db)
                .await
                .context("查询 request_execution 追踪记录失败")?
                .context("request_execution 追踪记录不存在")?
                .into();

        if let Some(upstream_request_id) = upstream_request_id {
            active.upstream_request_id = Set(upstream_request_id.to_string());
        }
        active.response_status_code = Set(response_status_code);
        active.response_body = Set(None);
        active.status = Set(RequestExecutionStatus::Succeeded);
        active.error_message = Set(String::new());
        active.duration_ms = Set(duration_ms);
        active.first_token_ms = Set(first_token_ms);
        active.finished_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active
            .update(&self.db)
            .await
            .context("更新 request_execution 流式成功记录失败")?;
        Ok(())
    }

    pub async fn finish_execution_stream_failure(
        &self,
        execution_id: i64,
        upstream_request_id: Option<&str>,
        response_status_code: i32,
        error_message: &str,
        response_body: Option<serde_json::Value>,
        duration_ms: i32,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let mut active: request_execution::ActiveModel =
            request_execution::Entity::find_by_id(execution_id)
                .one(&self.db)
                .await
                .context("查询 request_execution 失败记录失败")?
                .context("request_execution 失败记录不存在")?
                .into();

        if let Some(upstream_request_id) = upstream_request_id {
            active.upstream_request_id = Set(upstream_request_id.to_string());
        }
        active.response_status_code = Set(response_status_code);
        active.response_body = Set(response_body);
        active.status = Set(RequestExecutionStatus::Failed);
        active.error_message = Set(error_message.to_string());
        active.duration_ms = Set(duration_ms);
        active.first_token_ms = Set(first_token_ms);
        active.finished_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active
            .update(&self.db)
            .await
            .context("更新 request_execution 流式失败记录失败")?;
        Ok(())
    }

    pub async fn finish_execution_trace_span_from_execution_success(
        &self,
        execution_id: i64,
        upstream_request_id: Option<&str>,
        response_status_code: i32,
        output_payload: serde_json::Value,
        duration_ms: i32,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let tracked_execution = request_execution::Entity::find_by_id(execution_id)
            .one(&self.db)
            .await
            .context("查询 request_execution trace_span 记录失败")?
            .context("request_execution trace_span 记录不存在")?;
        let tracked_request = request::Entity::find_by_id(tracked_execution.ai_request_id)
            .one(&self.db)
            .await
            .context("查询 request trace 记录失败")?
            .context("request trace 记录不存在")?;

        if tracked_request.trace_id <= 0 {
            return Ok(());
        }

        self.finish_execution_trace_span_success(
            tracked_request.trace_id,
            tracked_execution.attempt_no,
            output_payload,
            execution_trace_span_success_metadata(
                &tracked_execution,
                upstream_request_id,
                response_status_code,
                duration_ms,
                first_token_ms,
            ),
        )
        .await
    }

    pub async fn finish_execution_trace_span_from_execution_failure(
        &self,
        execution_id: i64,
        upstream_request_id: Option<&str>,
        response_status_code: i32,
        error_message: &str,
        output_payload: serde_json::Value,
        duration_ms: i32,
        first_token_ms: i32,
    ) -> ApiResult<()> {
        let tracked_execution = request_execution::Entity::find_by_id(execution_id)
            .one(&self.db)
            .await
            .context("查询 request_execution trace_span 记录失败")?
            .context("request_execution trace_span 记录不存在")?;
        let tracked_request = request::Entity::find_by_id(tracked_execution.ai_request_id)
            .one(&self.db)
            .await
            .context("查询 request trace 记录失败")?
            .context("request trace 记录不存在")?;

        if tracked_request.trace_id <= 0 {
            return Ok(());
        }

        self.finish_execution_trace_span_failure(
            tracked_request.trace_id,
            tracked_execution.attempt_no,
            error_message,
            output_payload,
            execution_trace_span_failure_metadata(
                &tracked_execution,
                upstream_request_id,
                response_status_code,
                error_message,
                duration_ms,
                first_token_ms,
            ),
        )
        .await
    }

    async fn finish_trace(
        &self,
        trace_id: i64,
        status: TraceStatus,
        metadata: serde_json::Value,
    ) -> ApiResult<()> {
        let mut active: trace::ActiveModel = trace::Entity::find_by_id(trace_id)
            .one(&self.db)
            .await
            .context("查询 trace 追踪记录失败")?
            .context("trace 追踪记录不存在")?
            .into();

        active.status = Set(status);
        active.metadata = Set(metadata);
        active.finished_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active
            .update(&self.db)
            .await
            .context("更新 trace 追踪记录失败")?;
        Ok(())
    }

    async fn finish_trace_span(
        &self,
        trace_id: i64,
        span_key: &str,
        status: TraceSpanStatus,
        output_payload: serde_json::Value,
        error_message: &str,
        metadata: serde_json::Value,
    ) -> ApiResult<()> {
        let mut active: trace_span::ActiveModel = trace_span::Entity::find()
            .filter(trace_span::Column::TraceId.eq(trace_id))
            .filter(trace_span::Column::SpanKey.eq(span_key))
            .one(&self.db)
            .await
            .context("查询 trace_span 追踪记录失败")?
            .context("trace_span 追踪记录不存在")?
            .into();

        active.status = Set(status);
        active.output_payload = Set(output_payload);
        active.error_message = Set(error_message.to_string());
        active.metadata = Set(metadata);
        active.finished_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active
            .update(&self.db)
            .await
            .context("更新 trace_span 追踪记录失败")?;
        Ok(())
    }
}

pub struct CreateTraceTracking<'a> {
    pub trace_key: &'a str,
    pub root_request_id: &'a str,
    pub user_id: i64,
    pub metadata: serde_json::Value,
}

impl CreateTraceTracking<'_> {
    pub fn into_active_model(self) -> trace::ActiveModel {
        trace::ActiveModel {
            trace_key: Set(self.trace_key.to_string()),
            root_request_id: Set(self.root_request_id.to_string()),
            user_id: Set(self.user_id),
            source_type: Set("request".to_string()),
            status: Set(TraceStatus::Running),
            metadata: Set(self.metadata),
            finished_at: Set(None),
            ..Default::default()
        }
    }
}

pub struct CreateTraceSpanTracking<'a> {
    pub trace_id: i64,
    pub parent_span_id: i64,
    pub span_key: &'a str,
    pub span_name: &'a str,
    pub span_type: &'a str,
    pub target_kind: &'a str,
    pub target_ref: &'a str,
    pub input_payload: serde_json::Value,
    pub metadata: serde_json::Value,
}

impl CreateTraceSpanTracking<'_> {
    pub fn into_active_model(self) -> trace_span::ActiveModel {
        trace_span::ActiveModel {
            trace_id: Set(self.trace_id),
            parent_span_id: Set(self.parent_span_id),
            span_key: Set(self.span_key.to_string()),
            span_name: Set(self.span_name.to_string()),
            span_type: Set(self.span_type.to_string()),
            target_kind: Set(self.target_kind.to_string()),
            target_ref: Set(self.target_ref.to_string()),
            status: Set(TraceSpanStatus::Running),
            input_payload: Set(self.input_payload),
            output_payload: Set(serde_json::json!({})),
            error_message: Set(String::new()),
            metadata: Set(self.metadata),
            finished_at: Set(None),
            ..Default::default()
        }
    }
}

pub struct CreateChatRequestTracking<'a> {
    pub request_id: &'a str,
    pub trace_id: i64,
    pub token_info: &'a TokenInfo,
    pub request: &'a ChatCompletionRequest,
    pub client_ip: &'a str,
    pub user_agent: &'a str,
    pub headers: &'a HeaderMap,
}

impl CreateChatRequestTracking<'_> {
    pub fn into_active_model(self) -> request::ActiveModel {
        request::ActiveModel {
            request_id: Set(self.request_id.to_string()),
            user_id: Set(self.token_info.user_id),
            token_id: Set(self.token_info.token_id),
            project_id: Set(self.token_info.project_id),
            conversation_id: Set(0),
            message_id: Set(0),
            session_id: Set(0),
            thread_id: Set(0),
            trace_id: Set(self.trace_id),
            channel_group: Set(self.token_info.group.clone()),
            source_type: Set("api".to_string()),
            endpoint: Set("/v1/chat/completions".to_string()),
            request_format: Set(CHAT_COMPLETIONS_FORMAT.to_string()),
            requested_model: Set(self.request.model.clone()),
            upstream_model: Set(String::new()),
            is_stream: Set(self.request.stream),
            client_ip: Set(self.client_ip.to_string()),
            user_agent: Set(self.user_agent.to_string()),
            request_headers: Set(snapshot_headers(self.headers)),
            request_body: Set(
                serde_json::to_value(self.request).unwrap_or_else(|_| serde_json::json!({}))
            ),
            response_body: Set(None),
            response_status_code: Set(0),
            status: Set(RequestStatus::Processing),
            error_message: Set(String::new()),
            duration_ms: Set(0),
            first_token_ms: Set(0),
            ..Default::default()
        }
    }
}

pub struct CreateChatExecutionTracking<'a> {
    pub ai_request_id: i64,
    pub request_id: &'a str,
    pub attempt_no: i32,
    pub request: &'a ChatCompletionRequest,
    pub channel_id: i64,
    pub account_id: i64,
    pub upstream_model: &'a str,
    pub request_body: serde_json::Value,
}

impl CreateChatExecutionTracking<'_> {
    pub fn into_active_model(self) -> request_execution::ActiveModel {
        request_execution::ActiveModel {
            ai_request_id: Set(self.ai_request_id),
            request_id: Set(self.request_id.to_string()),
            attempt_no: Set(self.attempt_no.max(1)),
            channel_id: Set(self.channel_id),
            account_id: Set(self.account_id),
            endpoint: Set("/v1/chat/completions".to_string()),
            request_format: Set(CHAT_COMPLETIONS_FORMAT.to_string()),
            requested_model: Set(self.request.model.clone()),
            upstream_model: Set(self.upstream_model.to_string()),
            upstream_request_id: Set(String::new()),
            request_headers: Set(serde_json::json!({})),
            request_body: Set(self.request_body),
            response_body: Set(None),
            response_status_code: Set(0),
            status: Set(RequestExecutionStatus::Running),
            error_message: Set(String::new()),
            duration_ms: Set(0),
            first_token_ms: Set(0),
            finished_at: Set(None),
            ..Default::default()
        }
    }
}

pub fn snapshot_headers(headers: &HeaderMap) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers {
        let lower = name.as_str().to_ascii_lowercase();
        let is_sensitive = matches!(
            lower.as_str(),
            "authorization"
                | "proxy-authorization"
                | "x-api-key"
                | "api-key"
                | "cookie"
                | "set-cookie"
        );
        let rendered = if is_sensitive {
            "***".to_string()
        } else {
            value
                .to_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|_| String::from_utf8_lossy(value.as_bytes()).into_owned())
        };
        map.insert(
            name.as_str().to_string(),
            serde_json::Value::String(rendered),
        );
    }
    serde_json::Value::Object(map)
}

pub struct CreateEmbeddingsRequestTracking<'a> {
    pub request_id: &'a str,
    pub trace_id: i64,
    pub token_info: &'a TokenInfo,
    pub request: &'a EmbeddingRequest,
    pub client_ip: &'a str,
    pub user_agent: &'a str,
    pub headers: &'a HeaderMap,
}

impl CreateEmbeddingsRequestTracking<'_> {
    pub fn into_active_model(self) -> request::ActiveModel {
        request::ActiveModel {
            request_id: Set(self.request_id.to_string()),
            user_id: Set(self.token_info.user_id),
            token_id: Set(self.token_info.token_id),
            project_id: Set(self.token_info.project_id),
            conversation_id: Set(0),
            message_id: Set(0),
            session_id: Set(0),
            thread_id: Set(0),
            trace_id: Set(self.trace_id),
            channel_group: Set(self.token_info.group.clone()),
            source_type: Set("api".to_string()),
            endpoint: Set(EMBEDDINGS_ENDPOINT.to_string()),
            request_format: Set(EMBEDDINGS_FORMAT.to_string()),
            requested_model: Set(self.request.model.clone()),
            upstream_model: Set(String::new()),
            is_stream: Set(false),
            client_ip: Set(self.client_ip.to_string()),
            user_agent: Set(self.user_agent.to_string()),
            request_headers: Set(snapshot_headers(self.headers)),
            request_body: Set(
                serde_json::to_value(self.request).unwrap_or_else(|_| serde_json::json!({}))
            ),
            response_body: Set(None),
            response_status_code: Set(0),
            status: Set(RequestStatus::Processing),
            error_message: Set(String::new()),
            duration_ms: Set(0),
            first_token_ms: Set(0),
            ..Default::default()
        }
    }
}

pub struct CreateEmbeddingsExecutionTracking<'a> {
    pub ai_request_id: i64,
    pub request_id: &'a str,
    pub attempt_no: i32,
    pub request: &'a EmbeddingRequest,
    pub channel_id: i64,
    pub account_id: i64,
    pub upstream_model: &'a str,
    pub request_body: serde_json::Value,
}

impl CreateEmbeddingsExecutionTracking<'_> {
    pub fn into_active_model(self) -> request_execution::ActiveModel {
        request_execution::ActiveModel {
            ai_request_id: Set(self.ai_request_id),
            request_id: Set(self.request_id.to_string()),
            attempt_no: Set(self.attempt_no.max(1)),
            channel_id: Set(self.channel_id),
            account_id: Set(self.account_id),
            endpoint: Set(EMBEDDINGS_ENDPOINT.to_string()),
            request_format: Set(EMBEDDINGS_FORMAT.to_string()),
            requested_model: Set(self.request.model.clone()),
            upstream_model: Set(self.upstream_model.to_string()),
            upstream_request_id: Set(String::new()),
            request_headers: Set(serde_json::json!({})),
            request_body: Set(self.request_body),
            response_body: Set(None),
            response_status_code: Set(0),
            status: Set(RequestExecutionStatus::Running),
            error_message: Set(String::new()),
            duration_ms: Set(0),
            first_token_ms: Set(0),
            finished_at: Set(None),
            ..Default::default()
        }
    }
}

pub struct CreateResponsesRequestTracking<'a> {
    pub request_id: &'a str,
    pub trace_id: i64,
    pub token_info: &'a TokenInfo,
    pub request: &'a ResponsesRequest,
    pub client_ip: &'a str,
    pub user_agent: &'a str,
    pub headers: &'a HeaderMap,
}

impl CreateResponsesRequestTracking<'_> {
    pub fn into_active_model(self) -> request::ActiveModel {
        request::ActiveModel {
            request_id: Set(self.request_id.to_string()),
            user_id: Set(self.token_info.user_id),
            token_id: Set(self.token_info.token_id),
            project_id: Set(self.token_info.project_id),
            conversation_id: Set(0),
            message_id: Set(0),
            session_id: Set(0),
            thread_id: Set(0),
            trace_id: Set(self.trace_id),
            channel_group: Set(self.token_info.group.clone()),
            source_type: Set("api".to_string()),
            endpoint: Set(RESPONSES_ENDPOINT.to_string()),
            request_format: Set(RESPONSES_FORMAT.to_string()),
            requested_model: Set(self.request.model.clone()),
            upstream_model: Set(String::new()),
            is_stream: Set(self.request.stream),
            client_ip: Set(self.client_ip.to_string()),
            user_agent: Set(self.user_agent.to_string()),
            request_headers: Set(snapshot_headers(self.headers)),
            request_body: Set(
                serde_json::to_value(self.request).unwrap_or_else(|_| serde_json::json!({}))
            ),
            response_body: Set(None),
            response_status_code: Set(0),
            status: Set(RequestStatus::Processing),
            error_message: Set(String::new()),
            duration_ms: Set(0),
            first_token_ms: Set(0),
            ..Default::default()
        }
    }
}

pub struct CreateResponsesExecutionTracking<'a> {
    pub ai_request_id: i64,
    pub request_id: &'a str,
    pub attempt_no: i32,
    pub request: &'a ResponsesRequest,
    pub channel_id: i64,
    pub account_id: i64,
    pub upstream_model: &'a str,
    pub request_body: serde_json::Value,
}

impl CreateResponsesExecutionTracking<'_> {
    pub fn into_active_model(self) -> request_execution::ActiveModel {
        request_execution::ActiveModel {
            ai_request_id: Set(self.ai_request_id),
            request_id: Set(self.request_id.to_string()),
            attempt_no: Set(self.attempt_no.max(1)),
            channel_id: Set(self.channel_id),
            account_id: Set(self.account_id),
            endpoint: Set(RESPONSES_ENDPOINT.to_string()),
            request_format: Set(RESPONSES_FORMAT.to_string()),
            requested_model: Set(self.request.model.clone()),
            upstream_model: Set(self.upstream_model.to_string()),
            upstream_request_id: Set(String::new()),
            request_headers: Set(serde_json::json!({})),
            request_body: Set(self.request_body),
            response_body: Set(None),
            response_status_code: Set(0),
            status: Set(RequestExecutionStatus::Running),
            error_message: Set(String::new()),
            duration_ms: Set(0),
            first_token_ms: Set(0),
            finished_at: Set(None),
            ..Default::default()
        }
    }
}

pub struct CreateRetryAttemptTracking<'a> {
    pub domain_code: &'a str,
    pub task_type: &'a str,
    pub reference_id: &'a str,
    pub request_id: &'a str,
    pub attempt_no: i32,
    pub backoff_seconds: i32,
    pub error_message: &'a str,
    pub payload: serde_json::Value,
    pub next_retry_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl CreateRetryAttemptTracking<'_> {
    pub fn into_active_model(self) -> retry_attempt::ActiveModel {
        retry_attempt::ActiveModel {
            domain_code: Set(self.domain_code.to_string()),
            task_type: Set(self.task_type.to_string()),
            reference_id: Set(self.reference_id.to_string()),
            request_id: Set(self.request_id.to_string()),
            attempt_no: Set(self.attempt_no.max(1)),
            status: Set(RetryAttemptStatus::PendingRetry),
            backoff_seconds: Set(self.backoff_seconds.max(0)),
            error_message: Set(self.error_message.to_string()),
            payload: Set(self.payload),
            next_retry_at: Set(self.next_retry_at),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue::Set;
    use summer_ai_core::types::chat::ChatCompletionRequest;
    use summer_ai_core::types::embedding::EmbeddingRequest;
    use summer_ai_core::types::responses::ResponsesRequest;
    use summer_ai_model::entity::request::RequestStatus;
    use summer_ai_model::entity::request_execution::RequestExecutionStatus;
    use summer_ai_model::entity::retry_attempt::RetryAttemptStatus;
    use summer_ai_model::entity::trace::TraceStatus;
    use summer_ai_model::entity::trace_span::TraceSpanStatus;
    use summer_web::axum::http::{HeaderMap, HeaderValue, header::AUTHORIZATION};

    use crate::service::token::TokenInfo;

    use super::{
        CHAT_COMPLETIONS_FORMAT, CreateChatExecutionTracking, CreateChatRequestTracking,
        CreateEmbeddingsExecutionTracking, CreateEmbeddingsRequestTracking,
        CreateResponsesExecutionTracking, CreateResponsesRequestTracking,
        CreateRetryAttemptTracking, CreateTraceSpanTracking, CreateTraceTracking,
        EMBEDDINGS_ENDPOINT, EMBEDDINGS_FORMAT, RESPONSES_ENDPOINT, RESPONSES_FORMAT,
        snapshot_headers,
    };

    fn sample_request() -> ChatCompletionRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": false
        }))
        .expect("sample chat request")
    }

    fn token_info() -> TokenInfo {
        TokenInfo {
            token_id: 12,
            user_id: 34,
            project_id: 56,
            service_account_id: 78,
            name: "demo".into(),
            group: "vip".into(),
            remain_quota: 100,
            unlimited_quota: false,
            rpm_limit: 0,
            tpm_limit: 0,
            concurrency_limit: 0,
            allowed_models: vec!["gpt-4o".into()],
            endpoint_scopes: vec!["chat".into()],
        }
    }

    fn sample_responses_request() -> ResponsesRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4",
            "input": "hello",
            "stream": false
        }))
        .expect("sample responses request")
    }

    fn sample_embeddings_request() -> EmbeddingRequest {
        serde_json::from_value(serde_json::json!({
            "model": "text-embedding-3-small",
            "input": ["hello", "world"]
        }))
        .expect("sample embeddings request")
    }

    #[test]
    fn build_request_active_model_uses_chat_tracking_defaults() {
        let request = sample_request();
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        headers.insert("x-request-id", HeaderValue::from_static("req-test"));
        let model = CreateChatRequestTracking {
            request_id: "req-1",
            trace_id: 88,
            token_info: &token_info(),
            request: &request,
            client_ip: "127.0.0.1",
            user_agent: "codex-test",
            headers: &headers,
        }
        .into_active_model();

        assert_eq!(model.request_id, Set("req-1".to_string()));
        assert_eq!(model.endpoint, Set("/v1/chat/completions".to_string()));
        assert_eq!(
            model.request_format,
            Set(CHAT_COMPLETIONS_FORMAT.to_string())
        );
        assert_eq!(model.trace_id, Set(88));
        assert_eq!(model.requested_model, Set("gpt-4o".to_string()));
        assert_eq!(model.status, Set(RequestStatus::Processing));
        assert_eq!(model.user_id, Set(34));
        assert_eq!(model.token_id, Set(12));
        assert_eq!(model.channel_group, Set("vip".to_string()));
        assert_eq!(model.client_ip, Set("127.0.0.1".to_string()));
        assert_eq!(model.user_agent, Set("codex-test".to_string()));
        assert_eq!(
            model.request_headers,
            Set(serde_json::json!({
                "authorization": "***",
                "x-request-id": "req-test"
            }))
        );
    }

    #[test]
    fn build_request_execution_active_model_uses_first_attempt_defaults() {
        let request = sample_request();
        let upstream_body = serde_json::json!({"model": "gpt-4o-upstream"});
        let model = CreateChatExecutionTracking {
            ai_request_id: 11,
            request_id: "req-1",
            attempt_no: 2,
            request: &request,
            channel_id: 21,
            account_id: 31,
            upstream_model: "gpt-4o-upstream",
            request_body: upstream_body.clone(),
        }
        .into_active_model();

        assert_eq!(model.ai_request_id, Set(11));
        assert_eq!(model.request_id, Set("req-1".to_string()));
        assert_eq!(model.attempt_no, Set(2));
        assert_eq!(model.channel_id, Set(21));
        assert_eq!(model.account_id, Set(31));
        assert_eq!(model.upstream_model, Set("gpt-4o-upstream".to_string()));
        assert_eq!(model.request_body, Set(upstream_body));
        assert_eq!(model.status, Set(RequestExecutionStatus::Running));
    }

    #[test]
    fn build_responses_request_active_model_uses_responses_tracking_defaults() {
        let request = sample_responses_request();
        let headers = HeaderMap::new();
        let model = CreateResponsesRequestTracking {
            request_id: "resp-1",
            trace_id: 99,
            token_info: &token_info(),
            request: &request,
            client_ip: "127.0.0.1",
            user_agent: "codex-test",
            headers: &headers,
        }
        .into_active_model();

        assert_eq!(model.endpoint, Set(RESPONSES_ENDPOINT.to_string()));
        assert_eq!(model.request_format, Set(RESPONSES_FORMAT.to_string()));
        assert_eq!(model.trace_id, Set(99));
        assert_eq!(model.requested_model, Set("gpt-5.4".to_string()));
        assert_eq!(model.status, Set(RequestStatus::Processing));
    }

    #[test]
    fn build_responses_execution_active_model_uses_responses_defaults() {
        let request = sample_responses_request();
        let request_body = serde_json::json!({"model": "gpt-5.4", "input": "hello"});
        let model = CreateResponsesExecutionTracking {
            ai_request_id: 22,
            request_id: "resp-1",
            attempt_no: 3,
            request: &request,
            channel_id: 930011,
            account_id: 930011,
            upstream_model: "gpt-5.4",
            request_body: request_body.clone(),
        }
        .into_active_model();

        assert_eq!(model.endpoint, Set(RESPONSES_ENDPOINT.to_string()));
        assert_eq!(model.request_format, Set(RESPONSES_FORMAT.to_string()));
        assert_eq!(model.attempt_no, Set(3));
        assert_eq!(model.request_body, Set(request_body));
        assert_eq!(model.status, Set(RequestExecutionStatus::Running));
    }

    #[test]
    fn build_embeddings_request_active_model_uses_embeddings_tracking_defaults() {
        let request = sample_embeddings_request();
        let headers = HeaderMap::new();
        let model = CreateEmbeddingsRequestTracking {
            request_id: "embedding-1",
            trace_id: 66,
            token_info: &token_info(),
            request: &request,
            client_ip: "127.0.0.1",
            user_agent: "codex-test",
            headers: &headers,
        }
        .into_active_model();

        assert_eq!(model.endpoint, Set(EMBEDDINGS_ENDPOINT.to_string()));
        assert_eq!(model.request_format, Set(EMBEDDINGS_FORMAT.to_string()));
        assert_eq!(model.trace_id, Set(66));
        assert_eq!(
            model.requested_model,
            Set("text-embedding-3-small".to_string())
        );
        assert_eq!(model.status, Set(RequestStatus::Processing));
    }

    #[test]
    fn build_embeddings_execution_active_model_uses_embeddings_defaults() {
        let request = sample_embeddings_request();
        let request_body =
            serde_json::json!({"model": "text-embedding-3-small", "input": ["hello", "world"]});
        let model = CreateEmbeddingsExecutionTracking {
            ai_request_id: 33,
            request_id: "embedding-1",
            attempt_no: 4,
            request: &request,
            channel_id: 930011,
            account_id: 930011,
            upstream_model: "text-embedding-3-small",
            request_body: request_body.clone(),
        }
        .into_active_model();

        assert_eq!(model.endpoint, Set(EMBEDDINGS_ENDPOINT.to_string()));
        assert_eq!(model.request_format, Set(EMBEDDINGS_FORMAT.to_string()));
        assert_eq!(model.attempt_no, Set(4));
        assert_eq!(model.request_body, Set(request_body));
        assert_eq!(model.status, Set(RequestExecutionStatus::Running));
    }

    #[test]
    fn build_retry_attempt_active_model_uses_pending_retry_defaults() {
        let payload = serde_json::json!({"endpoint": "chat", "channelId": 12});
        let model = CreateRetryAttemptTracking {
            domain_code: "relay",
            task_type: "chat",
            reference_id: "req_123",
            request_id: "req_123",
            attempt_no: 1,
            backoff_seconds: 0,
            error_message: "upstream timeout",
            payload: payload.clone(),
            next_retry_at: None,
        }
        .into_active_model();

        assert_eq!(model.domain_code, Set("relay".to_string()));
        assert_eq!(model.task_type, Set("chat".to_string()));
        assert_eq!(model.reference_id, Set("req_123".to_string()));
        assert_eq!(model.request_id, Set("req_123".to_string()));
        assert_eq!(model.attempt_no, Set(1));
        assert_eq!(model.status, Set(RetryAttemptStatus::PendingRetry));
        assert_eq!(model.backoff_seconds, Set(0));
        assert_eq!(model.error_message, Set("upstream timeout".to_string()));
        assert_eq!(model.payload, Set(payload));
    }

    #[test]
    fn build_trace_active_model_uses_running_defaults() {
        let model = CreateTraceTracking {
            trace_key: "trace_123",
            root_request_id: "req_123",
            user_id: 34,
            metadata: serde_json::json!({
                "endpoint": "/v1/chat/completions",
                "requested_model": "gpt-4o"
            }),
        }
        .into_active_model();

        assert_eq!(model.trace_key, Set("trace_123".to_string()));
        assert_eq!(model.root_request_id, Set("req_123".to_string()));
        assert_eq!(model.user_id, Set(34));
        assert_eq!(model.source_type, Set("request".to_string()));
        assert_eq!(model.status, Set(TraceStatus::Running));
    }

    #[test]
    fn build_trace_span_active_model_uses_running_defaults() {
        let model = CreateTraceSpanTracking {
            trace_id: 88,
            parent_span_id: 0,
            span_key: "execution:1",
            span_name: "chat upstream attempt #1",
            span_type: "llm",
            target_kind: "channel_account",
            target_ref: "12:34",
            input_payload: serde_json::json!({"model": "gpt-4o"}),
            metadata: serde_json::json!({"attempt_no": 1}),
        }
        .into_active_model();

        assert_eq!(model.trace_id, Set(88));
        assert_eq!(model.parent_span_id, Set(0));
        assert_eq!(model.span_key, Set("execution:1".to_string()));
        assert_eq!(model.span_name, Set("chat upstream attempt #1".to_string()));
        assert_eq!(model.target_kind, Set("channel_account".to_string()));
        assert_eq!(model.status, Set(TraceSpanStatus::Running));
    }

    #[test]
    fn snapshot_headers_masks_sensitive_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        headers.insert("x-api-key", HeaderValue::from_static("sk-demo"));
        headers.insert("x-request-id", HeaderValue::from_static("req-123"));

        assert_eq!(
            snapshot_headers(&headers),
            serde_json::json!({
                "authorization": "***",
                "x-api-key": "***",
                "x-request-id": "req-123"
            })
        );
    }
}
