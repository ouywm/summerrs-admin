use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use summer::plugin::Service;
use summer_ai_model::dto::model_config::{
    CreateModelConfigDto, ModelConfigQueryDto, UpdateModelConfigDto,
};
use summer_ai_model::entity::billing::model_config::{self};
use summer_ai_model::entity::routing::{channel, channel_account, channel_model_price, vendor};
use summer_ai_model::vo::model_config::ModelConfigVo;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct ModelConfigService {
    #[inject(component)]
    db: DbConn,
}

impl ModelConfigService {
    pub async fn list(
        &self,
        query: ModelConfigQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ModelConfigVo>> {
        let page: Page<model_config::Model> = model_config::Entity::find()
            .filter(query)
            .order_by_asc(model_config::Column::ModelName)
            .order_by_asc(model_config::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询模型配置列表失败")?;

        Ok(page.map(ModelConfigVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<ModelConfigVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(ModelConfigVo::from_model(model))
    }

    pub async fn create(&self, dto: CreateModelConfigDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        self.ensure_unique_model_name(&dto.model_name).await?;
        self.ensure_vendor_exists(&dto.vendor_code).await?;
        dto.into_active_model(operator)
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建模型配置失败")?;
        Ok(())
    }

    pub async fn update(
        &self,
        id: i64,
        dto: UpdateModelConfigDto,
        operator: &str,
    ) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        if let Some(vendor_code) = dto.vendor_code.as_deref() {
            self.ensure_vendor_exists(vendor_code).await?;
        }

        let model = self.find_model_by_id(id).await?;
        let mut active: model_config::ActiveModel = model.into();
        dto.apply_to(&mut active, operator)
            .map_err(ApiErrors::BadRequest)?;
        active.update(&self.db).await.context("更新模型配置失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;

        let channel_refs = channel::Entity::find()
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("检查渠道模型引用失败")?
            .into_iter()
            .filter(|row| channel_references_model(row, &model.model_name))
            .count() as u64;
        let channel_account_refs = channel_account::Entity::find()
            .filter(channel_account::Column::DeletedAt.is_null())
            .filter(channel_account::Column::TestModel.eq(&model.model_name))
            .count(&self.db)
            .await
            .context("检查渠道账号测速模型引用失败")?;
        let channel_model_price_refs = channel_model_price::Entity::find()
            .filter(channel_model_price::Column::ModelName.eq(&model.model_name))
            .count(&self.db)
            .await
            .context("检查渠道价格模型引用失败")?;

        ensure_no_model_config_references(
            channel_refs,
            channel_account_refs,
            channel_model_price_refs,
        )
        .map_err(ApiErrors::Conflict)?;

        model_config::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除模型配置失败")?;
        Ok(())
    }

    async fn ensure_unique_model_name(&self, model_name: &str) -> ApiResult<()> {
        let exists = model_config::Entity::find()
            .filter(model_config::Column::ModelName.eq(model_name))
            .one(&self.db)
            .await
            .context("检查模型标识唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "模型标识已存在: model_name={model_name}"
            )));
        }
        Ok(())
    }

    async fn ensure_vendor_exists(&self, vendor_code: &str) -> ApiResult<()> {
        let exists = vendor::Entity::find()
            .filter(vendor::Column::VendorCode.eq(vendor_code))
            .one(&self.db)
            .await
            .context("检查供应商是否存在失败")?;
        if exists.is_none() {
            return Err(ApiErrors::BadRequest(format!(
                "供应商不存在: vendor_code={vendor_code}"
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<model_config::Model> {
        model_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询模型配置详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("模型配置不存在: id={id}")))
    }
}

pub fn ensure_no_model_config_references(
    channel_refs: u64,
    channel_account_refs: u64,
    channel_model_price_refs: u64,
) -> Result<(), String> {
    if channel_refs == 0 && channel_account_refs == 0 && channel_model_price_refs == 0 {
        return Ok(());
    }
    Err(format!(
        "模型配置仍被引用，不能删除: 渠道={channel_refs}, 渠道账号={channel_account_refs}, 渠道价格={channel_model_price_refs}"
    ))
}

fn channel_references_model(channel: &channel::Model, model_name: &str) -> bool {
    channel.test_model == model_name
        || json_string_array_contains(&channel.models, model_name)
        || channel
            .model_mapping
            .as_object()
            .is_some_and(|mapping| mapping.contains_key(model_name))
}

fn json_string_array_contains(value: &serde_json::Value, expected: &str) -> bool {
    value
        .as_array()
        .is_some_and(|arr| arr.iter().any(|item| item.as_str() == Some(expected)))
}
