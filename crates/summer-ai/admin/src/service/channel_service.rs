use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_ai_model::dto::channel::{ChannelQueryDto, CreateChannelDto, UpdateChannelDto};
use summer_ai_model::entity::routing::channel::{self, ChannelStatus};
use summer_ai_model::vo::channel::{ChannelDetailVo, ChannelStatusCountsVo, ChannelVo};
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct ChannelService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelService {
    // ─── 增 ───────────────────────────────────────────────────────────

    pub async fn create(&self, dto: CreateChannelDto, operator: &str) -> ApiResult<()> {
        let active = dto.into_active_model(operator);
        active.insert(&self.db).await.context("创建渠道失败")?;
        Ok(())
    }

    // ─── 删 ───────────────────────────────────────────────────────────

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        let mut active: channel::ActiveModel = model.into();
        active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active.update(&self.db).await.context("删除渠道失败")?;
        Ok(())
    }

    pub async fn batch_delete(&self, ids: Vec<i64>) -> ApiResult<u64> {
        if ids.is_empty() {
            return Err(ApiErrors::BadRequest("ids 不能为空".to_string()));
        }
        let now = chrono::Utc::now().fixed_offset();
        // 逐条软删除
        let mut count = 0u64;
        for id in ids {
            if let Ok(model) = self.find_model_by_id(id).await {
                let mut active: channel::ActiveModel = model.into();
                active.deleted_at = Set(Some(now));
                if active.update(&self.db).await.is_ok() {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    // ─── 改 ───────────────────────────────────────────────────────────

    pub async fn update(&self, id: i64, dto: UpdateChannelDto, operator: &str) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        let mut active: channel::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);
        active.update(&self.db).await.context("更新渠道失败")?;
        Ok(())
    }

    // ─── 查: 列表 ─────────────────────────────────────────────────────

    pub async fn list(
        &self,
        query: ChannelQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelVo>> {
        let id_sort = query.id_sort.unwrap_or(false);
        let mut select = channel::Entity::find().filter(query);

        if id_sort {
            select = select.order_by_desc(channel::Column::Id);
        } else {
            select = select
                .order_by_desc(channel::Column::Priority)
                .order_by_asc(channel::Column::Id);
        }

        let page: Page<channel::Model> = select
            .page(&self.db, &pagination)
            .await
            .context("查询渠道列表失败")?;

        Ok(page.map(ChannelVo::from_model))
    }

    // ─── 查: 详情 ─────────────────────────────────────────────────────

    pub async fn detail(&self, id: i64) -> ApiResult<ChannelDetailVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(ChannelDetailVo::from_model(model))
    }

    // ─── 查: 状态统计 ─────────────────────────────────────────────────

    pub async fn status_counts(
        &self,
        channel_type: Option<i16>,
    ) -> ApiResult<ChannelStatusCountsVo> {
        let mut select = channel::Entity::find().filter(channel::Column::DeletedAt.is_null());

        if let Some(t) = channel_type {
            select = select.filter(channel::Column::ChannelType.eq(t));
        }

        let all = select.all(&self.db).await.context("查询渠道状态统计失败")?;

        let total = all.len() as i64;
        let enabled = all
            .iter()
            .filter(|m| m.status == ChannelStatus::Enabled)
            .count() as i64;
        let manual_disabled = all
            .iter()
            .filter(|m| m.status == ChannelStatus::ManualDisabled)
            .count() as i64;
        let auto_disabled = all
            .iter()
            .filter(|m| m.status == ChannelStatus::AutoDisabled)
            .count() as i64;
        let archived = all
            .iter()
            .filter(|m| m.status == ChannelStatus::Archived)
            .count() as i64;

        Ok(ChannelStatusCountsVo {
            enabled,
            manual_disabled,
            auto_disabled,
            archived,
            total,
        })
    }

    // ─── 内部辅助 ─────────────────────────────────────────────────────

    async fn find_model_by_id(&self, id: i64) -> ApiResult<channel::Model> {
        channel::Entity::find_by_id(id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("渠道不存在: id={id}")))
    }
}
