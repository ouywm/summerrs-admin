use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, LoaderTrait, QueryFilter, QueryOrder, Set,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::request::QueryRequestDto;
use summer_ai_model::entity::request::{self, RequestStatus};
use summer_ai_model::entity::request_execution::{self, ExecutionStatus};
use summer_ai_model::vo::request::{
    RequestDetailVo, RequestExecutionVo, RequestVo, RequestWithExecutionsVo,
};

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

impl RequestService {
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
