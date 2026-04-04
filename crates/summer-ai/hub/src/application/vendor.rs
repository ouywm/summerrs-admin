use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::vendor::{CreateVendorDto, QueryVendorDto, UpdateVendorDto};
use summer_ai_model::entity::vendor;
use summer_ai_model::vo::vendor::VendorVo;

#[derive(Clone, Service)]
pub struct VendorService {
    #[inject(component)]
    db: DbConn,
}

impl VendorService {
    pub async fn list_vendors(
        &self,
        query: QueryVendorDto,
        pagination: Pagination,
    ) -> ApiResult<Page<VendorVo>> {
        let page = vendor::Entity::find()
            .filter(query)
            .order_by_asc(vendor::Column::VendorSort)
            .order_by_asc(vendor::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询供应商列表失败")?;

        Ok(page.map(VendorVo::from_model))
    }

    pub async fn get_vendor(&self, id: i64) -> ApiResult<VendorVo> {
        let model = vendor::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询供应商失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("供应商不存在".to_string()))?;
        Ok(VendorVo::from_model(model))
    }

    pub async fn create_vendor(&self, dto: CreateVendorDto, operator: &str) -> ApiResult<VendorVo> {
        let model = dto
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建供应商失败")
            .map_err(ApiErrors::Internal)?;
        Ok(VendorVo::from_model(model))
    }

    pub async fn update_vendor(
        &self,
        id: i64,
        dto: UpdateVendorDto,
        operator: &str,
    ) -> ApiResult<VendorVo> {
        let model = vendor::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询供应商失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("供应商不存在".to_string()))?;

        let mut active: vendor::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);

        let updated = active
            .update(&self.db)
            .await
            .context("更新供应商失败")
            .map_err(ApiErrors::Internal)?;
        Ok(VendorVo::from_model(updated))
    }

    pub async fn delete_vendor(&self, id: i64) -> ApiResult<()> {
        vendor::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除供应商失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    /// 获取所有启用的供应商（给前端下拉选择）
    pub async fn list_all_enabled(&self) -> ApiResult<Vec<VendorVo>> {
        let vendors = vendor::Entity::find()
            .filter(vendor::Column::Enabled.eq(true))
            .order_by_asc(vendor::Column::VendorSort)
            .all(&self.db)
            .await
            .context("查询启用供应商失败")
            .map_err(ApiErrors::Internal)?;
        Ok(vendors.into_iter().map(VendorVo::from_model).collect())
    }
}
