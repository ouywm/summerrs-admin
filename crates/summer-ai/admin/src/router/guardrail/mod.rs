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

use crate::router::guardrail::req::{
    CreateGuardrailConfigReq, CreateGuardrailRuleReq, CreatePromptProtectionRuleReq,
    GuardrailRuleQuery, GuardrailViolationQuery, UpdateGuardrailConfigReq, UpdateGuardrailRuleReq,
    UpdatePromptProtectionRuleReq,
};
use crate::router::guardrail::res::{
    GuardrailConfigRes, GuardrailMetricDailyRes, GuardrailRuleRes, GuardrailViolationRes,
    PromptProtectionRuleRes,
};
use crate::service::guardrail::GuardrailService;

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_configs)
        .typed_route(get_config)
        .typed_route(create_config)
        .typed_route(update_config)
        .typed_route(delete_config)
        .typed_route(list_rules)
        .typed_route(get_rule)
        .typed_route(create_rule)
        .typed_route(update_rule)
        .typed_route(delete_rule)
        .typed_route(list_violations)
        .typed_route(list_prompt_rules)
        .typed_route(create_prompt_rule)
        .typed_route(update_prompt_rule)
        .typed_route(delete_prompt_rule)
        .typed_route(list_metric_daily)
}

#[get_api("/ai/guardrail/config")]
pub async fn list_configs(
    Component(svc): Component<GuardrailService>,
) -> ApiResult<Json<Vec<GuardrailConfigRes>>> {
    Ok(Json(svc.list_configs().await?))
}

#[get_api("/ai/guardrail/config/{id}")]
pub async fn get_config(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<GuardrailConfigRes>> {
    Ok(Json(svc.get_config(id).await?))
}

#[post_api("/ai/guardrail/config")]
pub async fn create_config(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    ValidatedJson(req): ValidatedJson<CreateGuardrailConfigReq>,
) -> ApiResult<Json<GuardrailConfigRes>> {
    Ok(Json(svc.create_config(req, &profile.nick_name).await?))
}

#[put_api("/ai/guardrail/config/{id}")]
pub async fn update_config(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
    ValidatedJson(req): ValidatedJson<UpdateGuardrailConfigReq>,
) -> ApiResult<Json<GuardrailConfigRes>> {
    Ok(Json(svc.update_config(id, req, &profile.nick_name).await?))
}

#[delete_api("/ai/guardrail/config/{id}")]
pub async fn delete_config(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_config(id).await
}

#[get_api("/ai/guardrail/rule")]
pub async fn list_rules(
    Component(svc): Component<GuardrailService>,
    Query(query): Query<GuardrailRuleQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<GuardrailRuleRes>>> {
    Ok(Json(svc.list_rules(query, pagination).await?))
}

#[get_api("/ai/guardrail/rule/{id}")]
pub async fn get_rule(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<GuardrailRuleRes>> {
    Ok(Json(svc.get_rule(id).await?))
}

#[post_api("/ai/guardrail/rule")]
pub async fn create_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    ValidatedJson(req): ValidatedJson<CreateGuardrailRuleReq>,
) -> ApiResult<Json<GuardrailRuleRes>> {
    Ok(Json(svc.create_rule(req, &profile.nick_name).await?))
}

#[put_api("/ai/guardrail/rule/{id}")]
pub async fn update_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
    ValidatedJson(req): ValidatedJson<UpdateGuardrailRuleReq>,
) -> ApiResult<Json<GuardrailRuleRes>> {
    Ok(Json(svc.update_rule(id, req, &profile.nick_name).await?))
}

#[delete_api("/ai/guardrail/rule/{id}")]
pub async fn delete_rule(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_rule(id).await
}

#[get_api("/ai/guardrail/violation")]
pub async fn list_violations(
    Component(svc): Component<GuardrailService>,
    Query(query): Query<GuardrailViolationQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<GuardrailViolationRes>>> {
    Ok(Json(svc.list_violations(query, pagination).await?))
}

#[get_api("/ai/guardrail/prompt-protection")]
pub async fn list_prompt_rules(
    Component(svc): Component<GuardrailService>,
    pagination: Pagination,
) -> ApiResult<Json<Page<PromptProtectionRuleRes>>> {
    Ok(Json(svc.list_prompt_rules(pagination).await?))
}

#[post_api("/ai/guardrail/prompt-protection")]
pub async fn create_prompt_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    ValidatedJson(req): ValidatedJson<CreatePromptProtectionRuleReq>,
) -> ApiResult<Json<PromptProtectionRuleRes>> {
    Ok(Json(svc.create_prompt_rule(req, &profile.nick_name).await?))
}

#[put_api("/ai/guardrail/prompt-protection/{id}")]
pub async fn update_prompt_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
    ValidatedJson(req): ValidatedJson<UpdatePromptProtectionRuleReq>,
) -> ApiResult<Json<PromptProtectionRuleRes>> {
    Ok(Json(
        svc.update_prompt_rule(id, req, &profile.nick_name).await?,
    ))
}

#[delete_api("/ai/guardrail/prompt-protection/{id}")]
pub async fn delete_prompt_rule(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_prompt_rule(id).await
}

#[get_api("/ai/guardrail/metric-daily")]
pub async fn list_metric_daily(
    Component(svc): Component<GuardrailService>,
    pagination: Pagination,
) -> ApiResult<Json<Page<GuardrailMetricDailyRes>>> {
    Ok(Json(svc.list_metric_daily(pagination).await?))
}
