use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::channel_account::req::{
    ChannelAccountQuery, CreateChannelAccountReq, UpdateChannelAccountReq,
};
use crate::router::channel_account::res::ChannelAccountRes;
use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel_account::{self, ChannelAccountStatus};

#[derive(Clone, Service)]
pub struct ChannelAccountService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelAccountService {
    pub async fn list_accounts(
        &self,
        query: ChannelAccountQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelAccountRes>> {
        let page = channel_account::Entity::find()
            .filter(query)
            .order_by_desc(channel_account::Column::Priority)
            .order_by_desc(channel_account::Column::Weight)
            .order_by_desc(channel_account::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询渠道账号列表失败")?;

        Ok(page.map(ChannelAccountRes::from_model))
    }

    pub async fn create_account(
        &self,
        req: CreateChannelAccountReq,
        operator: &str,
    ) -> ApiResult<ChannelAccountRes> {
        self.ensure_channel_exists(req.channel_id).await?;
        let model = req
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建渠道账号失败")?;
        Ok(ChannelAccountRes::from_model(model))
    }

    pub async fn update_account(
        &self,
        id: i64,
        req: UpdateChannelAccountReq,
        operator: &str,
    ) -> ApiResult<ChannelAccountRes> {
        if let Some(channel_id) = req.channel_id {
            self.ensure_channel_exists(channel_id).await?;
        }

        let mut active: channel_account::ActiveModel = channel_account::Entity::find_by_id(id)
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道账号失败")?
            .ok_or_else(|| ApiErrors::NotFound("渠道账号不存在".to_string()))?
            .into();

        req.apply_to(&mut active, operator);
        let model = active.update(&self.db).await.context("更新渠道账号失败")?;
        Ok(ChannelAccountRes::from_model(model))
    }

    pub async fn delete_account(&self, id: i64, operator: &str) -> ApiResult<()> {
        let mut active: channel_account::ActiveModel = channel_account::Entity::find_by_id(id)
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道账号失败")?
            .ok_or_else(|| ApiErrors::NotFound("渠道账号不存在".to_string()))?
            .into();

        active.status = Set(ChannelAccountStatus::Disabled);
        active.schedulable = Set(false);
        active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active.update_by = Set(operator.to_string());
        active.update(&self.db).await.context("删除渠道账号失败")?;
        Ok(())
    }

    async fn ensure_channel_exists(&self, channel_id: i64) -> ApiResult<()> {
        let exists = channel::Entity::find_by_id(channel_id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道失败")?
            .is_some();

        if exists {
            Ok(())
        } else {
            Err(ApiErrors::NotFound("渠道不存在".to_string()))
        }
    }
}
