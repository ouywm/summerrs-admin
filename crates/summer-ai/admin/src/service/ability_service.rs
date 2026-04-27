use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_model::dto::ability::{AbilityQueryDto, CreateAbilityDto, UpdateAbilityDto};
use summer_ai_model::entity::routing::{ability, channel};
use summer_ai_model::vo::ability::AbilityVo;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct AbilityService {
    #[inject(component)]
    db: DbConn,
}

impl AbilityService {
    pub async fn list(
        &self,
        query: AbilityQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<AbilityVo>> {
        let page: Page<ability::Model> = ability::Entity::find()
            .filter(query)
            .order_by_asc(ability::Column::ChannelGroup)
            .order_by_asc(ability::Column::EndpointScope)
            .order_by_asc(ability::Column::Model)
            .order_by_desc(ability::Column::Priority)
            .order_by_desc(ability::Column::Weight)
            .order_by_asc(ability::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询能力列表失败")?;

        Ok(page.map(AbilityVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<AbilityVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(AbilityVo::from_model(model))
    }

    pub async fn create(&self, dto: CreateAbilityDto) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        self.ensure_channel_consistency(dto.channel_id, &dto.channel_group)
            .await?;
        self.ensure_unique_ability_key(
            &dto.channel_group,
            &dto.endpoint_scope,
            &dto.model,
            dto.channel_id,
            None,
        )
        .await?;
        dto.into_active_model()
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建能力失败")?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateAbilityDto) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        dto.validate_business_rules(&model)
            .map_err(ApiErrors::BadRequest)?;

        let next_channel_group = dto
            .channel_group
            .clone()
            .unwrap_or_else(|| model.channel_group.clone());
        let next_endpoint_scope = dto
            .endpoint_scope
            .clone()
            .unwrap_or_else(|| model.endpoint_scope.clone());
        let next_model = dto.model.clone().unwrap_or_else(|| model.model.clone());
        let next_channel_id = dto.channel_id.unwrap_or(model.channel_id);

        self.ensure_channel_consistency(next_channel_id, &next_channel_group)
            .await?;
        self.ensure_unique_ability_key(
            &next_channel_group,
            &next_endpoint_scope,
            &next_model,
            next_channel_id,
            Some(id),
        )
        .await?;

        let mut active: ability::ActiveModel = model.into();
        dto.apply_to(&mut active).map_err(ApiErrors::BadRequest)?;
        active.update(&self.db).await.context("更新能力失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let _model = self.find_model_by_id(id).await?;
        ability::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除能力失败")?;
        Ok(())
    }

    async fn ensure_channel_consistency(
        &self,
        channel_id: i64,
        channel_group: &str,
    ) -> ApiResult<()> {
        let channel = self.find_channel_by_id(channel_id).await?;
        if channel.channel_group != channel_group {
            return Err(ApiErrors::BadRequest(format!(
                "能力分组必须与渠道分组一致: channel_id={channel_id}, channel_group={channel_group}, expected={}",
                channel.channel_group
            )));
        }
        Ok(())
    }

    async fn ensure_unique_ability_key(
        &self,
        channel_group: &str,
        endpoint_scope: &str,
        model: &str,
        channel_id: i64,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut select = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(channel_group))
            .filter(ability::Column::EndpointScope.eq(endpoint_scope))
            .filter(ability::Column::Model.eq(model))
            .filter(ability::Column::ChannelId.eq(channel_id));
        if let Some(exclude_id) = exclude_id {
            select = select.filter(ability::Column::Id.ne(exclude_id));
        }
        let exists = select.one(&self.db).await.context("检查能力唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "能力已存在: channel_group={channel_group}, endpoint_scope={endpoint_scope}, model={model}, channel_id={channel_id}"
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<ability::Model> {
        ability::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询能力详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("能力不存在: id={id}")))
    }

    async fn find_channel_by_id(&self, id: i64) -> ApiResult<channel::Model> {
        channel::Entity::find_by_id(id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询能力关联渠道失败")?
            .ok_or_else(|| ApiErrors::BadRequest(format!("渠道不存在: channel_id={id}")))
    }
}
