use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use summer::plugin::Service;
use summer_ai_model::dto::group_ratio::{
    CreateGroupRatioDto, GroupRatioQueryDto, UpdateGroupRatioDto,
};
use summer_ai_model::entity::billing::{group_ratio, token, user_quota};
use summer_ai_model::entity::routing::channel;
use summer_ai_model::vo::group_ratio::GroupRatioVo;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct GroupRatioService {
    #[inject(component)]
    db: DbConn,
}

impl GroupRatioService {
    pub async fn list(
        &self,
        query: GroupRatioQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<GroupRatioVo>> {
        let page: Page<group_ratio::Model> = group_ratio::Entity::find()
            .filter(query)
            .order_by_asc(group_ratio::Column::GroupCode)
            .page(&self.db, &pagination)
            .await
            .context("查询分组倍率列表失败")?;

        Ok(page.map(GroupRatioVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<GroupRatioVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(GroupRatioVo::from_model(model))
    }

    pub async fn create(&self, dto: CreateGroupRatioDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        self.ensure_unique_group_code(&dto.group_code).await?;
        dto.into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建分组倍率失败")?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateGroupRatioDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        let model = self.find_model_by_id(id).await?;
        let mut active: group_ratio::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);
        active.update(&self.db).await.context("更新分组倍率失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;

        let channel_refs = channel::Entity::find()
            .filter(channel::Column::DeletedAt.is_null())
            .filter(channel::Column::ChannelGroup.eq(&model.group_code))
            .count(&self.db)
            .await
            .context("检查渠道分组引用失败")?;
        let user_quota_refs = user_quota::Entity::find()
            .filter(user_quota::Column::ChannelGroup.eq(&model.group_code))
            .count(&self.db)
            .await
            .context("检查用户额度分组引用失败")?;
        let token_refs = token::Entity::find()
            .filter(token::Column::GroupCodeOverride.eq(&model.group_code))
            .count(&self.db)
            .await
            .context("检查令牌分组引用失败")?;
        let fallback_refs = group_ratio::Entity::find()
            .filter(group_ratio::Column::Id.ne(id))
            .filter(group_ratio::Column::FallbackGroupCode.eq(&model.group_code))
            .count(&self.db)
            .await
            .context("检查 fallback 分组引用失败")?;

        ensure_no_group_references(channel_refs, user_quota_refs, token_refs, fallback_refs)
            .map_err(ApiErrors::Conflict)?;

        group_ratio::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除分组倍率失败")?;
        Ok(())
    }

    async fn ensure_unique_group_code(&self, group_code: &str) -> ApiResult<()> {
        let exists = group_ratio::Entity::find()
            .filter(group_ratio::Column::GroupCode.eq(group_code))
            .one(&self.db)
            .await
            .context("检查分组编码唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "分组编码已存在: group_code={group_code}"
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<group_ratio::Model> {
        group_ratio::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询分组倍率详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("分组倍率不存在: id={id}")))
    }
}

pub fn ensure_no_group_references(
    channel_refs: u64,
    user_quota_refs: u64,
    token_refs: u64,
    fallback_refs: u64,
) -> Result<(), String> {
    if channel_refs == 0 && user_quota_refs == 0 && token_refs == 0 && fallback_refs == 0 {
        return Ok(());
    }
    Err(format!(
        "分组仍被引用，不能删除: 渠道={channel_refs}, 用户额度={user_quota_refs}, 令牌={token_refs}, fallback={fallback_refs}"
    ))
}
