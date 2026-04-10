use anyhow::Context;
use sea_orm::{EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::request_execution::req::RequestExecutionQuery;
use crate::router::request_execution::res::{RequestExecutionDetailRes, RequestExecutionListRes};
use summer_ai_model::entity::requests::request_execution;

#[derive(Clone, Service)]
pub struct RequestExecutionService {
    #[inject(component)]
    db: DbConn,
}

impl RequestExecutionService {
    pub async fn list_request_executions(
        &self,
        query: RequestExecutionQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<RequestExecutionListRes>> {
        let page = request_execution::Entity::find()
            .filter(query)
            .order_by_desc(request_execution::Column::StartedAt)
            .order_by_desc(request_execution::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询请求执行列表失败")?;

        Ok(page.map(RequestExecutionListRes::from_model))
    }

    pub async fn get_request_execution(&self, id: i64) -> ApiResult<RequestExecutionDetailRes> {
        let model = request_execution::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询请求执行详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("请求执行不存在".to_string()))?;

        Ok(RequestExecutionDetailRes::from_model(model))
    }
}
