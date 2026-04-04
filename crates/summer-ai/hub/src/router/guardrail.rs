use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use summer_ai_model::dto::guardrail::{
    CreateGuardrailConfigDto, CreateGuardrailRuleDto, CreatePromptProtectionRuleDto,
    QueryGuardrailRuleDto, QueryGuardrailViolationDto, UpdateGuardrailConfigDto,
    UpdateGuardrailRuleDto, UpdatePromptProtectionRuleDto,
};
use summer_ai_model::vo::guardrail::{
    GuardrailConfigVo, GuardrailRuleVo, GuardrailViolationVo, PromptProtectionRuleVo,
};

use crate::service::guardrail::GuardrailService;

// ─── Config ───

#[get_api("/ai/guardrail/config")]
pub async fn list_configs(
    Component(svc): Component<GuardrailService>,
) -> ApiResult<Json<Vec<GuardrailConfigVo>>> {
    let configs = svc.list_configs().await?;
    Ok(Json(configs))
}

#[get_api("/ai/guardrail/config/{id}")]
pub async fn get_config(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<GuardrailConfigVo>> {
    Ok(Json(svc.get_config(id).await?))
}

#[post_api("/ai/guardrail/config")]
pub async fn create_config(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    ValidatedJson(dto): ValidatedJson<CreateGuardrailConfigDto>,
) -> ApiResult<Json<GuardrailConfigVo>> {
    Ok(Json(svc.create_config(dto, &profile.nick_name).await?))
}

#[put_api("/ai/guardrail/config/{id}")]
pub async fn update_config(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateGuardrailConfigDto>,
) -> ApiResult<Json<GuardrailConfigVo>> {
    Ok(Json(svc.update_config(id, dto, &profile.nick_name).await?))
}

#[delete_api("/ai/guardrail/config/{id}")]
pub async fn delete_config(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_config(id).await
}

// ─── Rule ───

#[get_api("/ai/guardrail/rule")]
pub async fn list_rules(
    Component(svc): Component<GuardrailService>,
    Query(query): Query<QueryGuardrailRuleDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<GuardrailRuleVo>>> {
    Ok(Json(svc.list_rules(query, pagination).await?))
}

#[get_api("/ai/guardrail/rule/{id}")]
pub async fn get_rule(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<GuardrailRuleVo>> {
    Ok(Json(svc.get_rule(id).await?))
}

#[post_api("/ai/guardrail/rule")]
pub async fn create_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    ValidatedJson(dto): ValidatedJson<CreateGuardrailRuleDto>,
) -> ApiResult<Json<GuardrailRuleVo>> {
    Ok(Json(svc.create_rule(dto, &profile.nick_name).await?))
}

#[put_api("/ai/guardrail/rule/{id}")]
pub async fn update_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateGuardrailRuleDto>,
) -> ApiResult<Json<GuardrailRuleVo>> {
    Ok(Json(svc.update_rule(id, dto, &profile.nick_name).await?))
}

#[delete_api("/ai/guardrail/rule/{id}")]
pub async fn delete_rule(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_rule(id).await
}

// ─── Violation ───

#[get_api("/ai/guardrail/violation")]
pub async fn list_violations(
    Component(svc): Component<GuardrailService>,
    Query(query): Query<QueryGuardrailViolationDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<GuardrailViolationVo>>> {
    Ok(Json(svc.list_violations(query, pagination).await?))
}

// ─── Prompt Protection ───

#[get_api("/ai/guardrail/prompt-protection")]
pub async fn list_prompt_rules(
    Component(svc): Component<GuardrailService>,
    pagination: Pagination,
) -> ApiResult<Json<Page<PromptProtectionRuleVo>>> {
    Ok(Json(svc.list_prompt_rules(pagination).await?))
}

#[post_api("/ai/guardrail/prompt-protection")]
pub async fn create_prompt_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    ValidatedJson(dto): ValidatedJson<CreatePromptProtectionRuleDto>,
) -> ApiResult<Json<PromptProtectionRuleVo>> {
    Ok(Json(svc.create_prompt_rule(dto, &profile.nick_name).await?))
}

#[put_api("/ai/guardrail/prompt-protection/{id}")]
pub async fn update_prompt_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdatePromptProtectionRuleDto>,
) -> ApiResult<Json<PromptProtectionRuleVo>> {
    Ok(Json(
        svc.update_prompt_rule(id, dto, &profile.nick_name).await?,
    ))
}

#[delete_api("/ai/guardrail/prompt-protection/{id}")]
pub async fn delete_prompt_rule(
    Component(svc): Component<GuardrailService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_prompt_rule(id).await
}
