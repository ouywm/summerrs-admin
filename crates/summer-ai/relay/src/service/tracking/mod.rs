use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::Serialize;
use summer::plugin::Service;
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::embedding::EmbeddingRequest;
use summer_ai_core::types::responses::ResponsesRequest;
use summer_ai_model::entity::request::{self, RequestStatus};
use summer_ai_model::entity::request_execution::{self, RequestExecutionStatus};
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

#[derive(Clone, Service)]
pub struct TrackingService {
    #[inject(component)]
    db: DbConn,
}

impl TrackingService {
    pub async fn create_chat_request(
        &self,
        request_id: &str,
        token_info: &TokenInfo,
        request: &ChatCompletionRequest,
        client_ip: &str,
        user_agent: &str,
        headers: &HeaderMap,
    ) -> ApiResult<request::Model> {
        CreateChatRequestTracking {
            request_id,
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
        request: &ChatCompletionRequest,
        channel_id: i64,
        account_id: i64,
        upstream_model: &str,
        request_body: serde_json::Value,
    ) -> ApiResult<request_execution::Model> {
        CreateChatExecutionTracking {
            ai_request_id,
            request_id,
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
        token_info: &TokenInfo,
        request: &ResponsesRequest,
        client_ip: &str,
        user_agent: &str,
        headers: &HeaderMap,
    ) -> ApiResult<request::Model> {
        CreateResponsesRequestTracking {
            request_id,
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
        request: &ResponsesRequest,
        channel_id: i64,
        account_id: i64,
        upstream_model: &str,
        request_body: serde_json::Value,
    ) -> ApiResult<request_execution::Model> {
        CreateResponsesExecutionTracking {
            ai_request_id,
            request_id,
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
        token_info: &TokenInfo,
        request: &EmbeddingRequest,
        client_ip: &str,
        user_agent: &str,
        headers: &HeaderMap,
    ) -> ApiResult<request::Model> {
        CreateEmbeddingsRequestTracking {
            request_id,
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
        request: &EmbeddingRequest,
        channel_id: i64,
        account_id: i64,
        upstream_model: &str,
        request_body: serde_json::Value,
    ) -> ApiResult<request_execution::Model> {
        CreateEmbeddingsExecutionTracking {
            ai_request_id,
            request_id,
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
}

pub struct CreateChatRequestTracking<'a> {
    pub request_id: &'a str,
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
            project_id: Set(0),
            conversation_id: Set(0),
            message_id: Set(0),
            session_id: Set(0),
            thread_id: Set(0),
            trace_id: Set(0),
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
            attempt_no: Set(1),
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
            project_id: Set(0),
            conversation_id: Set(0),
            message_id: Set(0),
            session_id: Set(0),
            thread_id: Set(0),
            trace_id: Set(0),
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
            attempt_no: Set(1),
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
            project_id: Set(0),
            conversation_id: Set(0),
            message_id: Set(0),
            session_id: Set(0),
            thread_id: Set(0),
            trace_id: Set(0),
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
            attempt_no: Set(1),
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

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue::Set;
    use summer_ai_core::types::chat::ChatCompletionRequest;
    use summer_ai_core::types::embedding::EmbeddingRequest;
    use summer_ai_core::types::responses::ResponsesRequest;
    use summer_ai_model::entity::request::RequestStatus;
    use summer_ai_model::entity::request_execution::RequestExecutionStatus;
    use summer_web::axum::http::{HeaderMap, HeaderValue, header::AUTHORIZATION};

    use crate::service::token::TokenInfo;

    use super::{
        CHAT_COMPLETIONS_FORMAT, CreateChatExecutionTracking, CreateChatRequestTracking,
        CreateEmbeddingsExecutionTracking, CreateEmbeddingsRequestTracking,
        CreateResponsesExecutionTracking, CreateResponsesRequestTracking, EMBEDDINGS_ENDPOINT,
        EMBEDDINGS_FORMAT, RESPONSES_ENDPOINT, RESPONSES_FORMAT, snapshot_headers,
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
            request: &request,
            channel_id: 21,
            account_id: 31,
            upstream_model: "gpt-4o-upstream",
            request_body: upstream_body.clone(),
        }
        .into_active_model();

        assert_eq!(model.ai_request_id, Set(11));
        assert_eq!(model.request_id, Set("req-1".to_string()));
        assert_eq!(model.attempt_no, Set(1));
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
            token_info: &token_info(),
            request: &request,
            client_ip: "127.0.0.1",
            user_agent: "codex-test",
            headers: &headers,
        }
        .into_active_model();

        assert_eq!(model.endpoint, Set(RESPONSES_ENDPOINT.to_string()));
        assert_eq!(model.request_format, Set(RESPONSES_FORMAT.to_string()));
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
            request: &request,
            channel_id: 930011,
            account_id: 930011,
            upstream_model: "gpt-5.4",
            request_body: request_body.clone(),
        }
        .into_active_model();

        assert_eq!(model.endpoint, Set(RESPONSES_ENDPOINT.to_string()));
        assert_eq!(model.request_format, Set(RESPONSES_FORMAT.to_string()));
        assert_eq!(model.request_body, Set(request_body));
        assert_eq!(model.status, Set(RequestExecutionStatus::Running));
    }

    #[test]
    fn build_embeddings_request_active_model_uses_embeddings_tracking_defaults() {
        let request = sample_embeddings_request();
        let headers = HeaderMap::new();
        let model = CreateEmbeddingsRequestTracking {
            request_id: "embedding-1",
            token_info: &token_info(),
            request: &request,
            client_ip: "127.0.0.1",
            user_agent: "codex-test",
            headers: &headers,
        }
        .into_active_model();

        assert_eq!(model.endpoint, Set(EMBEDDINGS_ENDPOINT.to_string()));
        assert_eq!(model.request_format, Set(EMBEDDINGS_FORMAT.to_string()));
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
            request: &request,
            channel_id: 930011,
            account_id: 930011,
            upstream_model: "text-embedding-3-small",
            request_body: request_body.clone(),
        }
        .into_active_model();

        assert_eq!(model.endpoint, Set(EMBEDDINGS_ENDPOINT.to_string()));
        assert_eq!(model.request_format, Set(EMBEDDINGS_FORMAT.to_string()));
        assert_eq!(model.request_body, Set(request_body));
        assert_eq!(model.status, Set(RequestExecutionStatus::Running));
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
