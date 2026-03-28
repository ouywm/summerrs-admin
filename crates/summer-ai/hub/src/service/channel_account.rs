use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::channel_account::{
    CreateChannelAccountDto, QueryChannelAccountDto, UpdateChannelAccountDto,
};
use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_ai_model::vo::channel_account::ChannelAccountVo;

#[derive(Clone, Service)]
pub struct ChannelAccountService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelAccountService {
    pub async fn list_accounts(
        &self,
        query: QueryChannelAccountDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelAccountVo>> {
        let page = channel_account::Entity::find()
            .filter(query)
            .order_by_desc(channel_account::Column::Priority)
            .order_by_desc(channel_account::Column::Weight)
            .order_by_desc(channel_account::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("failed to list channel accounts")
            .map_err(ApiErrors::Internal)?;

        Ok(page.map(ChannelAccountVo::from_model))
    }

    pub async fn create_account(
        &self,
        dto: CreateChannelAccountDto,
        operator: &str,
    ) -> ApiResult<ChannelAccountVo> {
        self.ensure_channel_exists(dto.channel_id).await?;
        let model = dto
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("failed to create channel account")
            .map_err(ApiErrors::Internal)?;
        Ok(ChannelAccountVo::from_model(model))
    }

    pub async fn update_account(
        &self,
        id: i64,
        dto: UpdateChannelAccountDto,
        operator: &str,
    ) -> ApiResult<ChannelAccountVo> {
        if let Some(channel_id) = dto.channel_id {
            self.ensure_channel_exists(channel_id).await?;
        }

        let mut active: channel_account::ActiveModel = channel_account::Entity::find_by_id(id)
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("failed to query channel account")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("channel account not found".into()))?
            .into();

        dto.apply_to(&mut active, operator);
        let model = active
            .update(&self.db)
            .await
            .context("failed to update channel account")
            .map_err(ApiErrors::Internal)?;
        Ok(ChannelAccountVo::from_model(model))
    }

    pub async fn delete_account(&self, id: i64, operator: &str) -> ApiResult<()> {
        let mut active: channel_account::ActiveModel = channel_account::Entity::find_by_id(id)
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("failed to query channel account")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("channel account not found".into()))?
            .into();

        active.status = Set(AccountStatus::Disabled);
        active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active.update_by = Set(operator.to_string());
        active
            .update(&self.db)
            .await
            .context("failed to delete channel account")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    async fn ensure_channel_exists(&self, channel_id: i64) -> ApiResult<()> {
        let exists = channel::Entity::find_by_id(channel_id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("failed to query channel")
            .map_err(ApiErrors::Internal)?
            .is_some();

        if exists {
            Ok(())
        } else {
            Err(ApiErrors::NotFound("channel not found".into()))
        }
    }
}
