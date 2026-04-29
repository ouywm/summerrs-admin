use crate::service::vendor_service::VendorService;
use summer_admin_macros::log;
use summer_ai_model::dto::vendor::{CreateVendorDto, UpdateVendorDto, VendorQueryDto};
use summer_ai_model::vo::vendor::VendorVo;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

#[log(module = "ai/供应商管理", action = "查询供应商列表", biz_type = Query)]
#[get_api("/vendor/list")]
pub async fn list(
    Component(svc): Component<VendorService>,
    Query(query): Query<VendorQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<VendorVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/供应商管理", action = "查询供应商详情", biz_type = Query)]
#[get_api("/vendor/{id}")]
pub async fn detail(
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<VendorVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(module = "ai/供应商管理", action = "创建供应商", biz_type = Create)]
#[post_api("/vendor")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<VendorService>,
    ValidatedJson(dto): ValidatedJson<CreateVendorDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/供应商管理", action = "更新供应商", biz_type = Update)]
#[put_api("/vendor/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateVendorDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/供应商管理", action = "删除供应商", biz_type = Delete)]
#[delete_api("/vendor/{id}")]
pub async fn delete(
    Component(svc): Component<VendorService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
