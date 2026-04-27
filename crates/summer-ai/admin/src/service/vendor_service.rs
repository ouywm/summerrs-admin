use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use summer::plugin::Service;
use summer_ai_model::dto::vendor::{CreateVendorDto, UpdateVendorDto, VendorQueryDto};
use summer_ai_model::entity::billing::model_config;
use summer_ai_model::entity::operations::error_passthrough_rule;
use summer_ai_model::entity::routing::{channel, vendor};
use summer_ai_model::vo::vendor::VendorVo;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct VendorService {
    #[inject(component)]
    db: DbConn,
}

impl VendorService {
    pub async fn list(
        &self,
        query: VendorQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<VendorVo>> {
        let page: Page<vendor::Model> = vendor::Entity::find()
            .filter(query)
            .order_by_asc(vendor::Column::VendorSort)
            .order_by_asc(vendor::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询供应商列表失败")?;

        Ok(page.map(VendorVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<VendorVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(VendorVo::from_model(model))
    }

    pub async fn create(&self, dto: CreateVendorDto, operator: &str) -> ApiResult<()> {
        self.ensure_unique_vendor_code(&dto.vendor_code).await?;
        dto.into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建供应商失败")?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateVendorDto, operator: &str) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        let mut active: vendor::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);
        active.update(&self.db).await.context("更新供应商失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;

        let channel_refs = channel::Entity::find()
            .filter(channel::Column::DeletedAt.is_null())
            .filter(channel::Column::VendorCode.eq(&model.vendor_code))
            .count(&self.db)
            .await
            .context("检查渠道供应商引用失败")?;
        let model_config_refs = model_config::Entity::find()
            .filter(model_config::Column::VendorCode.eq(&model.vendor_code))
            .count(&self.db)
            .await
            .context("检查模型配置供应商引用失败")?;
        ensure_no_vendor_references(channel_refs, model_config_refs)
            .map_err(ApiErrors::Conflict)?;

        let error_rule_refs = error_passthrough_rule::Entity::find()
            .filter(error_passthrough_rule::Column::VendorCode.eq(&model.vendor_code))
            .count(&self.db)
            .await
            .context("检查错误透传规则供应商引用失败")?;
        if error_rule_refs > 0 {
            return Err(ApiErrors::Conflict(format!(
                "供应商仍被引用，不能删除: 错误透传规则={error_rule_refs}"
            )));
        }

        vendor::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除供应商失败")?;
        Ok(())
    }

    async fn ensure_unique_vendor_code(&self, vendor_code: &str) -> ApiResult<()> {
        let exists = vendor::Entity::find()
            .filter(vendor::Column::VendorCode.eq(vendor_code))
            .one(&self.db)
            .await
            .context("检查供应商编码唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "供应商编码已存在: vendor_code={vendor_code}"
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<vendor::Model> {
        vendor::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询供应商详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("供应商不存在: id={id}")))
    }
}

pub fn ensure_no_vendor_references(
    channel_refs: u64,
    model_config_refs: u64,
) -> Result<(), String> {
    if channel_refs == 0 && model_config_refs == 0 {
        return Ok(());
    }
    Err(format!(
        "供应商仍被引用，不能删除: 渠道={channel_refs}, 模型配置={model_config_refs}"
    ))
}
