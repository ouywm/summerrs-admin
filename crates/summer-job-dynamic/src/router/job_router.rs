//! 动态调度系统 admin API。
//!
//! 路径前缀由 app 层 `nest("/api", ...)` 决定，本 crate 写相对路径。所有 handler
//! 通过 inventory 自动注册到 `summer-job-dynamic` group，由 `router_with_layers()`
//! 统一挂 JWT。开发期暂未挂 `#[has_perm]`，上线前需补回权限校验。

use std::sync::Arc;

use summer_auth::LoginUser;
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::dto::{
    AddDependencyDto, CreateJobDto, HandlerVo, JobDependencyListVo, JobDetailVo, JobQueryDto,
    JobRunQueryDto, JobRunVo, JobVo, TriggerJobDto, UpdateJobDto,
};
use crate::engine::{MetricsSnapshot, SchedulerMetrics};
use crate::service::{DependencyService, JobService};

// ---------------------------------------------------------------------------
// handler 注册表（registry 中可用 handler 列表，给前端下拉用）
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
// 监控
// ---------------------------------------------------------------------------

#[get_api("/scheduler/metrics")]
pub async fn get_metrics(
    Component(metrics): Component<Arc<SchedulerMetrics>>,
) -> ApiResult<Json<MetricsSnapshot>> {
    Ok(Json(metrics.snapshot()))
}

// ---------------------------------------------------------------------------
// 任务依赖（A 跑完成功 → 自动触发 B）
// ---------------------------------------------------------------------------

#[get_api("/scheduler/jobs/{id}/dependencies")]
pub async fn list_job_dependencies(
    Component(svc): Component<DependencyService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<JobDependencyListVo>> {
    Ok(Json(svc.list_for_job(id).await?))
}

#[post_api("/scheduler/jobs/{id}/dependencies")]
pub async fn add_job_dependency(
    Component(svc): Component<DependencyService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<AddDependencyDto>,
) -> ApiResult<Json<i64>> {
    let model = svc.add(id, dto.downstream_id, dto.on_state).await?;
    Ok(Json(model.id))
}

#[delete_api("/scheduler/jobs/{id}/dependencies/{dep_id}")]
pub async fn delete_job_dependency(
    Component(svc): Component<DependencyService>,
    Path((_id, dep_id)): Path<(i64, i64)>,
) -> ApiResult<Json<()>> {
    svc.remove(dep_id).await?;
    Ok(Json(()))
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

// ---------------------------------------------------------------------------
// 脚本试运行（编辑器调试用）
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, schemars::JsonSchema, validator::Validate)]
#[serde(rename_all = "camelCase")]
pub struct ScriptDryrunDto {
    /// 脚本引擎（当前只支持 "rhai"）
    pub engine: String,
    #[validate(length(min = 1, max = 100_000, message = "脚本长度必须 1-100000 字符"))]
    pub script: String,
    #[serde(default)]
    pub params: serde_json::Value,
    /// 超时毫秒，默认 5000，最大 30000
    pub timeout_ms: Option<u64>,
}

#[post_api("/scheduler/script/dryrun")]
pub async fn script_dryrun(
    ValidatedJson(dto): ValidatedJson<ScriptDryrunDto>,
) -> ApiResult<Json<crate::script::rhai_handler::DryrunResult>> {
    if dto.engine != "rhai" {
        return Err(ApiErrors::BadRequest(format!(
            "暂不支持脚本引擎: {}（当前仅 rhai）",
            dto.engine
        )));
    }
    let result = crate::script::rhai_handler::dryrun(dto.script, dto.params, dto.timeout_ms).await;
    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// 执行统计聚合（仪表盘 / 任务详情图表用）
// ---------------------------------------------------------------------------

#[get_api("/scheduler/stats/overview")]
pub async fn stats_overview(
    Component(svc): Component<crate::service::StatsService>,
    Query(query): Query<crate::service::stats_service::StatsQuery>,
) -> ApiResult<Json<crate::service::stats_service::StatsOverviewVo>> {
    Ok(Json(svc.overview(query.period).await?))
}

#[get_api("/scheduler/jobs/{id}/stats")]
pub async fn job_stats(
    Component(svc): Component<crate::service::StatsService>,
    Path(id): Path<i64>,
    Query(query): Query<crate::service::stats_service::StatsQuery>,
) -> ApiResult<Json<crate::service::stats_service::JobStatsVo>> {
    Ok(Json(svc.job_stats(id, query.period).await?))
}
