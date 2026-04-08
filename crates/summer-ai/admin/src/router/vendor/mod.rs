pub mod req;
pub mod res;

use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::vendor::VendorService;

use self::req::{CreateVendorReq, UpdateVendorReq, VendorQuery};
use self::res::VendorRes;

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_vendors)
        .typed_route(list_enabled_vendors)
        .typed_route(get_vendor)
        .typed_route(create_vendor)
        .typed_route(update_vendor)
        .typed_route(delete_vendor)
}

#[get_api("/ai/vendor/list")]
pub async fn list_vendors(
    Component(svc): Component<VendorService>,
    Query(query): Query<VendorQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<VendorRes>>> {
    let page = svc.list_vendors(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/vendor/enabled")]
pub async fn list_enabled_vendors(
    Component(svc): Component<VendorService>,
) -> ApiResult<Json<Vec<VendorRes>>> {
    let vendors = svc.list_enabled().await?;
    Ok(Json(vendors))
}

#[get_api("/ai/vendor/{id}")]
pub async fn get_vendor(
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<VendorRes>> {
    let vendor = svc.get_vendor(id).await?;
    Ok(Json(vendor))
}

#[post_api("/ai/vendor")]
pub async fn create_vendor(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<VendorService>,
    ValidatedJson(dto): ValidatedJson<CreateVendorReq>,
) -> ApiResult<Json<VendorRes>> {
    let vendor = svc.create_vendor(dto, &profile.nick_name).await?;
    Ok(Json(vendor))
}

#[put_api("/ai/vendor/{id}")]
pub async fn update_vendor(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateVendorReq>,
) -> ApiResult<Json<VendorRes>> {
    let vendor = svc.update_vendor(id, dto, &profile.nick_name).await?;
    Ok(Json(vendor))
}

#[delete_api("/ai/vendor/{id}")]
pub async fn delete_vendor(
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_vendor(id).await?;
    Ok(())
}
