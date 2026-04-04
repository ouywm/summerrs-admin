use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use summer_ai_model::dto::alert::{
    CreateAlertRuleDto, CreateAlertSilenceDto, QueryAlertEventDto, QueryAlertRuleDto,
    QueryDailyStatsDto, UpdateAlertRuleDto,
};
use summer_ai_model::vo::alert::{AlertEventVo, AlertRuleVo, AlertSilenceVo, DailyStatsVo};

use crate::service::alert::AlertService;

// ─── 告警规则 ───

#[get_api("/ai/alert/rule")]
pub async fn list_rules(
    Component(svc): Component<AlertService>,
    Query(query): Query<QueryAlertRuleDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<AlertRuleVo>>> {
    let page = svc.list_rules(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/alert/rule/{id}")]
pub async fn get_rule(
    Component(svc): Component<AlertService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertRuleVo>> {
    let vo = svc.get_rule(id).await?;
    Ok(Json(vo))
}

#[post_api("/ai/alert/rule")]
pub async fn create_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertService>,
    ValidatedJson(dto): ValidatedJson<CreateAlertRuleDto>,
) -> ApiResult<Json<AlertRuleVo>> {
    let vo = svc.create_rule(dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[put_api("/ai/alert/rule/{id}")]
pub async fn update_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateAlertRuleDto>,
) -> ApiResult<Json<AlertRuleVo>> {
    let vo = svc.update_rule(id, dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[delete_api("/ai/alert/rule/{id}")]
pub async fn delete_rule(
    Component(svc): Component<AlertService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_rule(id).await?;
    Ok(())
}

// ─── 告警事件 ───

#[get_api("/ai/alert/event")]
pub async fn list_events(
    Component(svc): Component<AlertService>,
    Query(query): Query<QueryAlertEventDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<AlertEventVo>>> {
    let page = svc.list_events(query, pagination).await?;
    Ok(Json(page))
}

#[put_api("/ai/alert/event/{id}/ack")]
pub async fn ack_event(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertEventVo>> {
    let vo = svc.ack_event(id, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[put_api("/ai/alert/event/{id}/resolve")]
pub async fn resolve_event(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertEventVo>> {
    let vo = svc.resolve_event(id, &profile.nick_name).await?;
    Ok(Json(vo))
}

// ─── 告警静默 ───

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SilenceQuery {
    pub rule_id: Option<i64>,
}

#[get_api("/ai/alert/silence")]
pub async fn list_silences(
    Component(svc): Component<AlertService>,
    Query(query): Query<SilenceQuery>,
) -> ApiResult<Json<Vec<AlertSilenceVo>>> {
    let silences = svc.list_silences(query.rule_id).await?;
    Ok(Json(silences))
}

#[post_api("/ai/alert/silence")]
pub async fn create_silence(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertService>,
    ValidatedJson(dto): ValidatedJson<CreateAlertSilenceDto>,
) -> ApiResult<Json<AlertSilenceVo>> {
    let vo = svc.create_silence(dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[delete_api("/ai/alert/silence/{id}")]
pub async fn delete_silence(
    Component(svc): Component<AlertService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_silence(id).await?;
    Ok(())
}

// ─── 日度统计 ───

#[get_api("/ai/daily-stats")]
pub async fn list_daily_stats(
    Component(svc): Component<AlertService>,
    Query(query): Query<QueryDailyStatsDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<DailyStatsVo>>> {
    let page = svc.list_daily_stats(query, pagination).await?;
    Ok(Json(page))
}
