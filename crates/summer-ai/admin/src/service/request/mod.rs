use anyhow::Context;
use sea_orm::{EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::request::req::RequestQuery;
use crate::router::request::res::{RequestDetailRes, RequestListRes};
use summer_ai_model::entity::requests::request;

#[derive(Clone, Service)]
pub struct RequestService {
    #[inject(component)]
    db: DbConn,
}

impl RequestService {
    pub async fn list_requests(
        &self,
        query: RequestQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<RequestListRes>> {
        let page = request::Entity::find()
            .filter(query)
            .order_by_desc(request::Column::CreateTime)
            .order_by_desc(request::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询请求列表失败")?;

        Ok(page.map(RequestListRes::from_model))
    }

    pub async fn get_request(&self, id: i64) -> ApiResult<RequestDetailRes> {
        let model = request::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询请求详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("请求不存在".to_string()))?;

        Ok(RequestDetailRes::from_model(model))
    }
}
