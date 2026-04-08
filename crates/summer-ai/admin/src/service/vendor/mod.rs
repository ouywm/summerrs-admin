use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::vendor::req::{CreateVendorReq, UpdateVendorReq, VendorQuery};
use crate::router::vendor::res::VendorRes;
use summer_ai_model::entity::vendor;

#[derive(Clone, Service)]
pub struct VendorService {
    #[inject(component)]
    db: DbConn,
}

impl VendorService {
    pub async fn list_vendors(
        &self,
        query: VendorQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<VendorRes>> {
        let page = vendor::Entity::find()
            .filter(query)
            .order_by_asc(vendor::Column::VendorSort)
            .order_by_asc(vendor::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询供应商列表失败")?;

        Ok(page.map(VendorRes::from_model))
    }

    pub async fn list_enabled(&self) -> ApiResult<Vec<VendorRes>> {
        let vendors = vendor::Entity::find()
            .filter(vendor::Column::Enabled.eq(true))
            .order_by_asc(vendor::Column::VendorSort)
            .all(&self.db)
            .await
            .context("查询启用供应商失败")?;
        Ok(vendors.into_iter().map(VendorRes::from_model).collect())
    }

    pub async fn get_vendor(&self, id: i64) -> ApiResult<VendorRes> {
        let model = vendor::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询供应商失败")?
            .ok_or_else(|| ApiErrors::NotFound("供应商不存在".to_string()))?;
        Ok(VendorRes::from_model(model))
    }

    pub async fn create_vendor(
        &self,
        req: CreateVendorReq,
        operator: &str,
    ) -> ApiResult<VendorRes> {
        let model = req
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建供应商失败")?;
        Ok(VendorRes::from_model(model))
    }

    pub async fn update_vendor(
        &self,
        id: i64,
        req: UpdateVendorReq,
        operator: &str,
    ) -> ApiResult<VendorRes> {
        let model = vendor::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询供应商失败")?
            .ok_or_else(|| ApiErrors::NotFound("供应商不存在".to_string()))?;

        let mut active: vendor::ActiveModel = model.into();
        req.apply_to(&mut active, operator);
        let updated = active.update(&self.db).await.context("更新供应商失败")?;
        Ok(VendorRes::from_model(updated))
    }

    pub async fn delete_vendor(&self, id: i64) -> ApiResult<()> {
        vendor::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除供应商失败")?;
        Ok(())
    }
}
