use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{get_api, post_api, put_api};

use crate::service::billing::BillingService;
use summer_ai_model::dto::billing::*;
use summer_ai_model::vo::billing::*;

// ─── SubscriptionPlan ───
#[get_api("/ai/subscription-plan")]
pub async fn list_plans(
    Component(svc): Component<BillingService>,
    Query(q): Query<QuerySubscriptionPlanDto>,
    p: Pagination,
) -> ApiResult<Json<Page<SubscriptionPlanVo>>> {
    Ok(Json(svc.list_plans(q, p).await?))
}
#[get_api("/ai/subscription-plan/{id}")]
pub async fn get_plan(
    Component(svc): Component<BillingService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<SubscriptionPlanVo>> {
    Ok(Json(svc.get_plan(id).await?))
}
#[post_api("/ai/subscription-plan")]
pub async fn create_plan(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<BillingService>,
    ValidatedJson(dto): ValidatedJson<CreateSubscriptionPlanDto>,
) -> ApiResult<Json<SubscriptionPlanVo>> {
    Ok(Json(svc.create_plan(dto, &profile.nick_name).await?))
}
#[put_api("/ai/subscription-plan/{id}")]
pub async fn update_plan(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<BillingService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateSubscriptionPlanDto>,
) -> ApiResult<Json<SubscriptionPlanVo>> {
    Ok(Json(svc.update_plan(id, dto, &profile.nick_name).await?))
}

// ─── Order ───
#[get_api("/ai/order")]
pub async fn list_orders(
    Component(svc): Component<BillingService>,
    Query(q): Query<QueryOrderDto>,
    p: Pagination,
) -> ApiResult<Json<Page<OrderVo>>> {
    Ok(Json(svc.list_orders(q, p).await?))
}
#[get_api("/ai/order/{id}")]
pub async fn get_order(
    Component(svc): Component<BillingService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<OrderVo>> {
    Ok(Json(svc.get_order(id).await?))
}

// ─── Topup ───
#[get_api("/ai/topup")]
pub async fn list_topups(
    Component(svc): Component<BillingService>,
    Query(q): Query<QueryTopupDto>,
    p: Pagination,
) -> ApiResult<Json<Page<TopupVo>>> {
    Ok(Json(svc.list_topups(q, p).await?))
}

// ─── Redemption ───
#[get_api("/ai/redemption")]
pub async fn list_redemptions(
    Component(svc): Component<BillingService>,
    Query(q): Query<QueryRedemptionDto>,
    p: Pagination,
) -> ApiResult<Json<Page<RedemptionVo>>> {
    Ok(Json(svc.list_redemptions(q, p).await?))
}
#[post_api("/ai/redemption/batch")]
pub async fn create_redemption_batch(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<BillingService>,
    ValidatedJson(dto): ValidatedJson<CreateRedemptionBatchDto>,
) -> ApiResult<Json<Vec<RedemptionVo>>> {
    Ok(Json(
        svc.create_redemption_batch(dto, &profile.nick_name).await?,
    ))
}
