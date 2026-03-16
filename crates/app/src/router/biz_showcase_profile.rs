//! Generated admin router skeleton.

use common::error::ApiResult;
use common::extractor::{Path, Query, ValidatedJson};
use common::response::Json;
use macros::log;
use model::dto::biz_showcase_profile::{
    CreateShowcaseProfileDto, ShowcaseProfileQueryDto, UpdateShowcaseProfileDto,
};
use model::vo::biz_showcase_profile::ShowcaseProfileVo;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::biz_showcase_profile_service::BizShowcaseProfileService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "展示档案", action = "查询列表", biz_type = Query)]
#[get_api("/showcase_profile/list")]
pub async fn list(
    Component(svc): Component<BizShowcaseProfileService>,
    Query(query): Query<ShowcaseProfileQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ShowcaseProfileVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "展示档案", action = "查询详情", biz_type = Query)]
#[get_api("/showcase_profile/{id}")]
pub async fn detail(
    Component(svc): Component<BizShowcaseProfileService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ShowcaseProfileVo>> {
    let item = svc.get_by_id(id).await?;
    Ok(Json(item))
}

#[log(module = "展示档案", action = "创建", biz_type = Create)]
#[post_api("/showcase_profile")]
pub async fn create(
    Component(svc): Component<BizShowcaseProfileService>,
    ValidatedJson(dto): ValidatedJson<CreateShowcaseProfileDto>,
) -> ApiResult<()> {
    svc.create(dto).await?;
    Ok(())
}

#[log(module = "展示档案", action = "更新", biz_type = Update)]
#[put_api("/showcase_profile/{id}")]
pub async fn update(
    Component(svc): Component<BizShowcaseProfileService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateShowcaseProfileDto>,
) -> ApiResult<()> {
    svc.update(id, dto).await?;
    Ok(())
}

#[log(module = "展示档案", action = "删除", biz_type = Delete)]
#[delete_api("/showcase_profile/{id}")]
pub async fn delete(
    Component(svc): Component<BizShowcaseProfileService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
