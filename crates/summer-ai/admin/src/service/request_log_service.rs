use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_model::dto::request_log::RequestLogQueryDto;
use summer_ai_model::entity::requests::{log, request};
use summer_ai_model::vo::request_log::{RequestDetailVo, RequestLogVo};
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct RequestLogService {
    #[inject(component)]
    db: DbConn,
}

impl RequestLogService {
    pub async fn list(
        &self,
        query: RequestLogQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RequestLogVo>> {
        let page: Page<log::Model> = log::Entity::find()
            .filter(query)
            .order_by_desc(log::Column::CreateTime)
            .order_by_desc(log::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询请求日志列表失败")?;

        Ok(page.map(RequestLogVo::from_log))
    }

    pub async fn log_detail(&self, id: i64) -> ApiResult<RequestLogVo> {
        let log_row = log::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询请求日志详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("请求日志不存在: id={id}")))?;

        let request_row = if log_row.request_id.is_empty() {
            None
        } else {
            request::Entity::find()
                .filter(request::Column::RequestId.eq(&log_row.request_id))
                .one(&self.db)
                .await
                .context("查询请求快照失败")?
        };

        Ok(RequestLogVo::from_log_and_request(log_row, request_row))
    }

    pub async fn request_detail(&self, id: i64) -> ApiResult<RequestDetailVo> {
        let request = request::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询请求快照详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("请求快照不存在: id={id}")))?;

        Ok(RequestDetailVo::from_model(request))
    }

    pub async fn request_detail_by_request_id(
        &self,
        request_id: String,
    ) -> ApiResult<RequestDetailVo> {
        let request = request::Entity::find()
            .filter(request::Column::RequestId.eq(&request_id))
            .one(&self.db)
            .await
            .context("按 request_id 查询请求快照失败")?
            .ok_or_else(|| {
                ApiErrors::NotFound(format!("请求快照不存在: request_id={request_id}"))
            })?;

        Ok(RequestDetailVo::from_model(request))
    }
}
