use anyhow::Context;
use sea_orm::{EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::retry_attempt::req::RetryAttemptQuery;
use crate::router::retry_attempt::res::{RetryAttemptDetailRes, RetryAttemptListRes};
use summer_ai_model::entity::requests::retry_attempt;

#[derive(Clone, Service)]
pub struct RetryAttemptService {
    #[inject(component)]
    db: DbConn,
}

impl RetryAttemptService {
    pub async fn list_retry_attempts(
        &self,
        query: RetryAttemptQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<RetryAttemptListRes>> {
        let page = retry_attempt::Entity::find()
            .filter(query)
            .order_by_desc(retry_attempt::Column::CreateTime)
            .order_by_desc(retry_attempt::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询重试记录列表失败")?;

        Ok(page.map(RetryAttemptListRes::from_model))
    }

    pub async fn get_retry_attempt(&self, id: i64) -> ApiResult<RetryAttemptDetailRes> {
        let model = retry_attempt::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询重试记录详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("重试记录不存在".to_string()))?;

        Ok(RetryAttemptDetailRes::from_model(model))
    }
}
