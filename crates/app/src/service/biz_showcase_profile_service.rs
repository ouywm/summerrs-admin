//! Generated admin service skeleton.

use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use model::dto::biz_showcase_profile::{ CreateShowcaseProfileDto, ShowcaseProfileQueryDto, UpdateShowcaseProfileDto };
use model::entity::biz_showcase_profile;
use model::vo::biz_showcase_profile::ShowcaseProfileVo;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};


#[derive(Clone, Service)]
pub struct BizShowcaseProfileService {
    #[inject(component)]
    db: DbConn,
}

impl BizShowcaseProfileService {
    pub async fn list(&self, query: ShowcaseProfileQueryDto, pagination: Pagination) -> ApiResult<Page<ShowcaseProfileVo>> {
        let page = biz_showcase_profile::Entity::find()
            .filter(query)
            .page(&self.db, &pagination)
            .await
            .context("查询展示档案列表失败")?;

        Ok(page.map(ShowcaseProfileVo::from))
    }

    pub async fn get_by_id(&self, id: i64) -> ApiResult<ShowcaseProfileVo> {
        let model = biz_showcase_profile::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询展示档案详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("展示档案不存在".to_string()))?;

        Ok(ShowcaseProfileVo::from(model))
    }

    pub async fn create(&self, dto: CreateShowcaseProfileDto) -> ApiResult<()> {
        let active: biz_showcase_profile::ActiveModel = dto.into();
        active.insert(&self.db).await.context("创建展示档案失败")?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateShowcaseProfileDto) -> ApiResult<()> {
        let model = biz_showcase_profile::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询展示档案详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("展示档案不存在".to_string()))?;

        let mut active: biz_showcase_profile::ActiveModel = model.into();
        dto.apply_to(&mut active);
        active.update(&self.db).await.context("更新展示档案失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let result = biz_showcase_profile::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除展示档案失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("展示档案不存在".to_string()));
        }

        Ok(())
    }
}
