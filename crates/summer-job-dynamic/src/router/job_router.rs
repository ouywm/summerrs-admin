//! 动态调度系统 admin API。
//!
//! 路径前缀由 app 层 `nest("/api", ...)` 决定，本 crate 写相对路径。所有 handler
//! 通过 inventory 自动注册到 `summer-job-dynamic` group，由 `router_with_layers()`
//! 统一挂 JWT。

use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::dto::{
    CreateJobDto, HandlerVo, JobDetailVo, JobQueryDto, JobRunQueryDto, JobRunVo, JobVo,
    TriggerJobDto, UpdateJobDto,
};
use crate::service::JobService;

// ---------------------------------------------------------------------------
// handler 注册表（给前端下拉用）
// ---------------------------------------------------------------------------

#[get_api("/scheduler/handlers")]
pub async fn list_handlers(
    Component(svc): Component<JobService>,
) -> ApiResult<Json<Vec<HandlerVo>>> {
    Ok(Json(svc.list_handlers()))
}

// ---------------------------------------------------------------------------
// 任务 CRUD
// ---------------------------------------------------------------------------

#[get_api("/scheduler/jobs")]
pub async fn list_jobs(
    Component(svc): Component<JobService>,
    Query(query): Query<JobQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<JobVo>>> {
    Ok(Json(svc.list_jobs(query, pagination).await?))
}

#[get_api("/scheduler/jobs/{id}")]
pub async fn get_job_detail(
    Component(svc): Component<JobService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<JobDetailVo>> {
    Ok(Json(svc.get_job_detail(id).await?))
}

#[post_api("/scheduler/jobs")]
pub async fn create_job(
    user: LoginUser,
    Component(svc): Component<JobService>,
    ValidatedJson(dto): ValidatedJson<CreateJobDto>,
) -> ApiResult<Json<JobDetailVo>> {
    let job = svc.create_job(dto, Some(user.login_id.user_id)).await?;
    Ok(Json(job))
}

#[put_api("/scheduler/jobs/{id}")]
pub async fn update_job(
    Component(svc): Component<JobService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateJobDto>,
) -> ApiResult<Json<JobDetailVo>> {
    Ok(Json(svc.update_job(id, dto).await?))
}

#[delete_api("/scheduler/jobs/{id}")]
pub async fn delete_job(
    Component(svc): Component<JobService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_job(id).await
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToggleEnabledDto {
    pub enabled: bool,
}

#[post_api("/scheduler/jobs/{id}/toggle")]
pub async fn toggle_job(
    Component(svc): Component<JobService>,
    Path(id): Path<i64>,
    Json(dto): Json<ToggleEnabledDto>,
) -> ApiResult<Json<JobDetailVo>> {
    Ok(Json(svc.toggle_enabled(id, dto.enabled).await?))
}

#[post_api("/scheduler/jobs/{id}/trigger")]
pub async fn trigger_job(
    user: LoginUser,
    Component(svc): Component<JobService>,
    Path(id): Path<i64>,
    Json(dto): Json<TriggerJobDto>,
) -> ApiResult<()> {
    svc.trigger_job(id, Some(user.login_id.user_id), dto.params_override)
        .await
}

// ---------------------------------------------------------------------------
// 执行记录
// ---------------------------------------------------------------------------

#[get_api("/scheduler/runs")]
pub async fn list_runs(
    Component(svc): Component<JobService>,
    Query(query): Query<JobRunQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<JobRunVo>>> {
    Ok(Json(svc.list_runs(query, pagination).await?))
}

#[get_api("/scheduler/runs/{id}")]
pub async fn get_run_detail(
    Component(svc): Component<JobService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<JobRunVo>> {
    Ok(Json(svc.get_run_detail(id).await?))
}

// ---------------------------------------------------------------------------
// 批量操作（前端列表多选用）
// ---------------------------------------------------------------------------

#[post_api("/scheduler/jobs/batch/toggle")]
pub async fn batch_toggle_jobs(
    Component(svc): Component<JobService>,
    ValidatedJson(dto): ValidatedJson<crate::dto::BatchToggleDto>,
) -> ApiResult<Json<crate::dto::BatchResultVo>> {
    Ok(Json(svc.batch_toggle(dto.ids, dto.enabled).await))
}

#[delete_api("/scheduler/jobs/batch")]
pub async fn batch_delete_jobs(
    Component(svc): Component<JobService>,
    ValidatedJson(dto): ValidatedJson<crate::dto::BatchIdsDto>,
) -> ApiResult<Json<crate::dto::BatchResultVo>> {
    Ok(Json(svc.batch_delete(dto.ids).await))
}

#[post_api("/scheduler/jobs/batch/trigger")]
pub async fn batch_trigger_jobs(
    user: LoginUser,
    Component(svc): Component<JobService>,
    ValidatedJson(dto): ValidatedJson<crate::dto::BatchIdsDto>,
) -> ApiResult<Json<crate::dto::BatchResultVo>> {
    Ok(Json(
        svc.batch_trigger(dto.ids, Some(user.login_id.user_id))
            .await,
    ))
}
