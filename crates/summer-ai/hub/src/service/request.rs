use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, LoaderTrait, QueryFilter, QueryOrder, Set,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use summer_web::axum::http::HeaderMap;

use summer_ai_model::dto::request::QueryRequestDto;
use summer_ai_model::entity::request::{self, RequestStatus};
use summer_ai_model::entity::request_execution::{self, ExecutionStatus};
use summer_ai_model::vo::request::{
    RequestDetailVo, RequestExecutionVo, RequestVo, RequestWithExecutionsVo,
};

use crate::relay::channel_router::SelectedChannel;
use crate::service::token::TokenInfo;

#[derive(Clone, Service)]
pub struct RequestService {
    #[inject(component)]
    db: DbConn,
}

#[derive(Debug)]
pub struct RequestStatusUpdate {
    pub status: RequestStatus,
    pub error_message: Option<String>,
    pub duration_ms: Option<i32>,
    pub first_token_ms: Option<i32>,
    pub response_status_code: Option<i32>,
    pub response_body: Option<serde_json::Value>,
    pub upstream_model: Option<String>,
}

#[derive(Debug)]
pub struct ExecutionStatusUpdate {
    pub status: ExecutionStatus,
    pub error_message: Option<String>,
    pub duration_ms: Option<i32>,
    pub first_token_ms: Option<i32>,
    pub response_status_code: Option<i32>,
    pub response_body: Option<serde_json::Value>,
    pub upstream_request_id: Option<String>,
}

pub struct RequestSnapshotInput<'a> {
    pub request_id: &'a str,
    pub token_info: &'a TokenInfo,
    pub endpoint: &'a str,
    pub request_format: &'a str,
    pub requested_model: &'a str,
    pub is_stream: bool,
    pub client_ip: &'a str,
    pub user_agent: &'a str,
    pub headers: &'a HeaderMap,
    pub request_body: serde_json::Value,
}

pub struct ExecutionSnapshotInput<'a> {
    pub ai_request_id: i64,
    pub request_id: &'a str,
    pub attempt_no: i32,
    pub channel: &'a SelectedChannel,
    pub endpoint: &'a str,
    pub request_format: &'a str,
    pub requested_model: &'a str,
    pub upstream_model: &'a str,
    pub request: &'a reqwest::Request,
    pub started_at: chrono::DateTime<chrono::FixedOffset>,
}

impl RequestService {
    pub async fn try_update_request_status(&self, id: Option<i64>, update: RequestStatusUpdate) {
        let Some(id) = id else {
            return;
        };
        if let Err(error) = self.update_request_status(id, update).await {
            tracing::warn!(error = %error, request_id = id, "failed to update AI request status");
        }
    }

    pub async fn try_update_execution_status(
        &self,
        id: Option<i64>,
        update: ExecutionStatusUpdate,
    ) {
        let Some(id) = id else {
            return;
        };
        if let Err(error) = self.update_execution_status(id, update).await {
            tracing::warn!(
                error = %error,
                execution_id = id,
                "failed to update AI request execution status"
            );
        }
    }

    /// 创建请求记录（请求进入时调用）
    pub async fn create_request(&self, model: request::ActiveModel) -> ApiResult<request::Model> {
        model
            .insert(&self.db)
            .await
            .context("创建 AI 请求记录失败")
            .map_err(ApiErrors::Internal)
    }

    /// 更新请求状态（请求完成或失败时调用）
    pub async fn update_request_status(
        &self,
        id: i64,
        update: RequestStatusUpdate,
    ) -> ApiResult<request::Model> {
        let mut active: request::ActiveModel = request::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询 AI 请求失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("请求不存在".to_string()))?
            .into();

        active.status = Set(update.status);
        if let Some(msg) = update.error_message {
            active.error_message = Set(msg);
        }
        if let Some(ms) = update.duration_ms {
            active.duration_ms = Set(ms);
        }
        if let Some(ms) = update.first_token_ms {
            active.first_token_ms = Set(ms);
        }
        if let Some(code) = update.response_status_code {
            active.response_status_code = Set(code);
        }
        if let Some(body) = update.response_body {
            active.response_body = Set(Some(body));
        }
        if let Some(model) = update.upstream_model {
            active.upstream_model = Set(model);
        }

        active
            .update(&self.db)
            .await
            .context("更新 AI 请求状态失败")
            .map_err(ApiErrors::Internal)
    }

    /// 记录执行尝试（每次上游转发时调用）
    pub async fn record_execution(
        &self,
        model: request_execution::ActiveModel,
    ) -> ApiResult<request_execution::Model> {
        model
            .insert(&self.db)
            .await
            .context("记录执行尝试失败")
            .map_err(ApiErrors::Internal)
    }

    /// 更新执行尝试状态
    pub async fn update_execution_status(
        &self,
        id: i64,
        update: ExecutionStatusUpdate,
    ) -> ApiResult<request_execution::Model> {
        let mut active: request_execution::ActiveModel = request_execution::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询执行尝试失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("执行尝试不存在".to_string()))?
            .into();

        active.status = Set(update.status);
        active.finished_at = Set(Some(chrono::Utc::now().fixed_offset()));
        if let Some(msg) = update.error_message {
            active.error_message = Set(msg);
        }
        if let Some(ms) = update.duration_ms {
            active.duration_ms = Set(ms);
        }
        if let Some(ms) = update.first_token_ms {
            active.first_token_ms = Set(ms);
        }
        if let Some(code) = update.response_status_code {
            active.response_status_code = Set(code);
        }
        if let Some(body) = update.response_body {
            active.response_body = Set(Some(body));
        }
        if let Some(rid) = update.upstream_request_id {
            active.upstream_request_id = Set(rid);
        }

        active
            .update(&self.db)
            .await
            .context("更新执行尝试状态失败")
            .map_err(ApiErrors::Internal)
    }

    /// 分页查询请求列表
    pub async fn query_requests(
        &self,
        query: QueryRequestDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RequestVo>> {
        let page = request::Entity::find()
            .filter(query)
            .order_by_desc(request::Column::CreateTime)
            .order_by_desc(request::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询 AI 请求列表失败")?;

        Ok(page.map(RequestVo::from_model))
    }

    /// 获取请求详情（含执行尝试列表）
    pub async fn get_request_detail(&self, id: i64) -> ApiResult<RequestWithExecutionsVo> {
        let req = request::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询 AI 请求详情失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("请求不存在".to_string()))?;

        let executions = vec![req.clone()]
            .load_many(request_execution::Entity, &self.db)
            .await
            .context("查询执行尝试列表失败")
            .map_err(ApiErrors::Internal)?
            .into_iter()
            .next()
            .unwrap_or_default();

        Ok(RequestWithExecutionsVo {
            request: RequestDetailVo::from_model(req),
            executions: executions
                .into_iter()
                .map(RequestExecutionVo::from_model)
                .collect(),
        })
    }

    /// 通过 request_id 获取请求详情
    pub async fn get_by_request_id(&self, request_id: &str) -> ApiResult<RequestWithExecutionsVo> {
        let req = request::Entity::find()
            .filter(request::Column::RequestId.eq(request_id))
            .one(&self.db)
            .await
            .context("查询 AI 请求详情失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("请求不存在".to_string()))?;

        let id = req.id;
        let executions = request_execution::Entity::find()
            .filter(request_execution::Column::AiRequestId.eq(id))
            .order_by_asc(request_execution::Column::AttemptNo)
            .all(&self.db)
            .await
            .context("查询执行尝试列表失败")
            .map_err(ApiErrors::Internal)?;

        Ok(RequestWithExecutionsVo {
            request: RequestDetailVo::from_model(req),
            executions: executions
                .into_iter()
                .map(RequestExecutionVo::from_model)
                .collect(),
        })
    }
}

pub fn build_request_active_model(input: RequestSnapshotInput<'_>) -> request::ActiveModel {
    let now = chrono::Utc::now().fixed_offset();
    request::ActiveModel {
        request_id: Set(input.request_id.to_string()),
        user_id: Set(input.token_info.user_id),
        token_id: Set(input.token_info.token_id),
        project_id: Set(0),
        conversation_id: Set(0),
        message_id: Set(0),
        session_id: Set(0),
        thread_id: Set(0),
        trace_id: Set(0),
        channel_group: Set(input.token_info.group.clone()),
        source_type: Set("api".to_string()),
        endpoint: Set(input.endpoint.to_string()),
        request_format: Set(input.request_format.to_string()),
        requested_model: Set(input.requested_model.to_string()),
        upstream_model: Set(String::new()),
        is_stream: Set(input.is_stream),
        client_ip: Set(input.client_ip.to_string()),
        user_agent: Set(input.user_agent.to_string()),
        request_headers: Set(snapshot_headers(input.headers)),
        request_body: Set(input.request_body),
        response_body: Set(None),
        response_status_code: Set(0),
        status: Set(RequestStatus::Processing),
        error_message: Set(String::new()),
        duration_ms: Set(0),
        first_token_ms: Set(0),
        create_time: Set(now),
        update_time: Set(now),
        ..Default::default()
    }
}

pub fn build_execution_active_model(
    input: ExecutionSnapshotInput<'_>,
) -> request_execution::ActiveModel {
    request_execution::ActiveModel {
        ai_request_id: Set(input.ai_request_id),
        request_id: Set(input.request_id.to_string()),
        attempt_no: Set(input.attempt_no),
        channel_id: Set(input.channel.channel_id),
        account_id: Set(input.channel.account_id),
        endpoint: Set(input.endpoint.to_string()),
        request_format: Set(input.request_format.to_string()),
        requested_model: Set(input.requested_model.to_string()),
        upstream_model: Set(input.upstream_model.to_string()),
        upstream_request_id: Set(String::new()),
        request_headers: Set(snapshot_headers(input.request.headers())),
        request_body: Set(snapshot_reqwest_request_body(input.request)),
        response_body: Set(None),
        response_status_code: Set(0),
        status: Set(ExecutionStatus::Running),
        error_message: Set(String::new()),
        duration_ms: Set(0),
        first_token_ms: Set(0),
        started_at: Set(input.started_at),
        finished_at: Set(None),
        create_time: Set(input.started_at),
        ..Default::default()
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

pub fn snapshot_reqwest_request_body(request: &reqwest::Request) -> serde_json::Value {
    request
        .body()
        .and_then(|body| body.as_bytes())
        .map(snapshot_response_body_bytes)
        .unwrap_or(serde_json::Value::Null)
}

pub fn snapshot_response_body_bytes(bytes: &[u8]) -> serde_json::Value {
    if bytes.is_empty() {
        return serde_json::Value::Null;
    }

    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(bytes).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_web::axum::http::{HeaderMap, HeaderValue, header};

    fn sample_token() -> TokenInfo {
        TokenInfo {
            token_id: 7,
            user_id: 9,
            name: "demo".into(),
            group: "default".into(),
            remain_quota: 1000,
            unlimited_quota: false,
            rpm_limit: 0,
            tpm_limit: 0,
            concurrency_limit: 0,
            allowed_models: Vec::new(),
            endpoint_scopes: Vec::new(),
        }
    }

    fn sample_channel() -> SelectedChannel {
        SelectedChannel {
            channel_id: 11,
            channel_name: "primary".into(),
            channel_type: 1,
            base_url: "https://api.example.com".into(),
            model_mapping: serde_json::json!({}),
            api_key: "sk-upstream".into(),
            account_id: 22,
            account_name: "acct".into(),
        }
    }

    #[test]
    fn snapshot_headers_redacts_sensitive_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        headers.insert(header::COOKIE, HeaderValue::from_static("session=abc"));
        headers.insert("x-api-key", HeaderValue::from_static("secret-key"));
        headers.insert("x-request-id", HeaderValue::from_static("req_123"));

        let snapshot = snapshot_headers(&headers);

        assert_eq!(snapshot["authorization"], "***");
        assert_eq!(snapshot["cookie"], "***");
        assert_eq!(snapshot["x-api-key"], "***");
        assert_eq!(snapshot["x-request-id"], "req_123");
    }

    #[test]
    fn snapshot_reqwest_request_body_parses_json_payload() {
        let client = reqwest::Client::new();
        let request = client
            .post("https://api.example.com/v1/chat/completions")
            .json(&serde_json::json!({"model": "gpt-5", "input": "hello"}))
            .build()
            .unwrap();

        let snapshot = snapshot_reqwest_request_body(&request);

        assert_eq!(snapshot["model"], "gpt-5");
        assert_eq!(snapshot["input"], "hello");
    }

    #[test]
    fn build_request_active_model_uses_processing_defaults() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static("req_123"));

        let model = build_request_active_model(RequestSnapshotInput {
            request_id: "req_123",
            token_info: &sample_token(),
            endpoint: "chat/completions",
            request_format: "openai/chat_completions",
            requested_model: "gpt-5",
            is_stream: true,
            client_ip: "127.0.0.1",
            user_agent: "test-agent",
            headers: &headers,
            request_body: serde_json::json!({"model": "gpt-5"}),
        });

        assert_eq!(model.request_id, Set("req_123".to_string()));
        assert_eq!(model.status, Set(RequestStatus::Processing));
        assert_eq!(model.response_status_code, Set(0));
        assert_eq!(
            model.request_headers,
            Set(serde_json::json!({"x-request-id": "req_123"}))
        );
    }

    #[test]
    fn build_execution_active_model_uses_channel_and_running_status() {
        let client = reqwest::Client::new();
        let request = client
            .post("https://api.example.com/v1/chat/completions")
            .header("x-api-key", "secret")
            .json(&serde_json::json!({"model": "gpt-5"}))
            .build()
            .unwrap();
        let started_at = chrono::Utc::now().fixed_offset();

        let model = build_execution_active_model(ExecutionSnapshotInput {
            ai_request_id: 88,
            request_id: "req_123",
            attempt_no: 2,
            channel: &sample_channel(),
            endpoint: "chat/completions",
            request_format: "openai/chat_completions",
            requested_model: "gpt-5",
            upstream_model: "gpt-5-mini",
            request: &request,
            started_at,
        });

        assert_eq!(model.ai_request_id, Set(88));
        assert_eq!(model.attempt_no, Set(2));
        assert_eq!(model.channel_id, Set(11));
        assert_eq!(model.account_id, Set(22));
        assert_eq!(model.status, Set(ExecutionStatus::Running));
        let Set(headers) = model.request_headers else {
            panic!("expected request headers to be set");
        };
        assert_eq!(headers["x-api-key"], serde_json::json!("***"));
        assert_eq!(
            model.request_body,
            Set(serde_json::json!({"model": "gpt-5"}))
        );
    }
}
