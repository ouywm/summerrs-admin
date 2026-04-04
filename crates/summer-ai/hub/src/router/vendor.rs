use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use summer_ai_model::dto::vendor::{CreateVendorDto, QueryVendorDto, UpdateVendorDto};
use summer_ai_model::vo::vendor::VendorVo;

use crate::service::vendor::VendorService;

#[get_api("/ai/vendor")]
pub async fn list_vendors(
    Component(svc): Component<VendorService>,
    Query(query): Query<QueryVendorDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<VendorVo>>> {
    let page = svc.list_vendors(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/vendor/enabled")]
pub async fn list_enabled_vendors(
    Component(svc): Component<VendorService>,
) -> ApiResult<Json<Vec<VendorVo>>> {
    let vendors = svc.list_all_enabled().await?;
    Ok(Json(vendors))
}

#[get_api("/ai/vendor/{id}")]
pub async fn get_vendor(
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<VendorVo>> {
    let vo = svc.get_vendor(id).await?;
    Ok(Json(vo))
}

#[post_api("/ai/vendor")]
pub async fn create_vendor(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<VendorService>,
    ValidatedJson(dto): ValidatedJson<CreateVendorDto>,
) -> ApiResult<Json<VendorVo>> {
    let vo = svc.create_vendor(dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[put_api("/ai/vendor/{id}")]
pub async fn update_vendor(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateVendorDto>,
) -> ApiResult<Json<VendorVo>> {
    let vo = svc.update_vendor(id, dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[delete_api("/ai/vendor/{id}")]
pub async fn delete_vendor(
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_vendor(id).await?;
    Ok(())
}
