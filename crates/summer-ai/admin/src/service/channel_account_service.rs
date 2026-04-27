use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_ai_model::dto::channel_account::{
    ChannelAccountQueryDto, CreateChannelAccountDto, UpdateChannelAccountDto,
};
use summer_ai_model::entity::routing::channel_account;
use summer_ai_model::vo::channel_account::{ChannelAccountDetailVo, ChannelAccountVo};
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct ChannelAccountService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelAccountService {
    pub async fn create(&self, dto: CreateChannelAccountDto, operator: &str) -> ApiResult<()> {
        let active = dto.into_active_model(operator);
        active.insert(&self.db).await.context("创建渠道账号失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        let mut active: channel_account::ActiveModel = model.into();
        active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active.update(&self.db).await.context("删除渠道账号失败")?;
        Ok(())
    }

    pub async fn update(
        &self,
        id: i64,
        dto: UpdateChannelAccountDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        let mut active: channel_account::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);
        active.update(&self.db).await.context("更新渠道账号失败")?;
        Ok(())
    }

    pub async fn list(
        &self,
        query: ChannelAccountQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelAccountVo>> {
        let page: Page<channel_account::Model> = channel_account::Entity::find()
            .filter(query)
            .order_by_asc(channel_account::Column::ChannelId)
            .order_by_desc(channel_account::Column::Priority)
            .order_by_asc(channel_account::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询渠道账号列表失败")?;
        Ok(page.map(ChannelAccountVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<ChannelAccountDetailVo> {
        let model = self.find_model_by_id(id).await?;
        let base = ChannelAccountVo::from_model(model.clone());
        Ok(ChannelAccountDetailVo {
            base,
            credentials: model.credentials,
            extra: model.extra,
            disabled_api_keys: model.disabled_api_keys,
            last_error_message: model.last_error_message,
        })
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<channel_account::Model> {
        channel_account::Entity::find_by_id(id)
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道账号详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("渠道账号不存在: id={id}")))
    }
}
