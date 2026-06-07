use summer_admin_macros::log;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_system_model::dto::job::{
    CreateJobDto, JobKeyQueryDto, JobListQueryDto, JobTaskQueryDto, TriggerJobDto, UpdateJobDto,
};
use summer_system_model::vo::job::{RatchJobTaskVo, RatchJobVo, RatchNamespaceVo};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::job_service::JobService;

#[log(module = "任务管理", action = "查询任务列表", biz_type = Query)]
#[get_api("/job/list")]
pub async fn list_jobs(
    Component(svc): Component<JobService>,
    Query(query): Query<JobListQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RatchJobVo>>> {
    let vo = svc.list_jobs(query, pagination).await?;
    Ok(Json(vo))
}

#[log(module = "任务管理", action = "获取任务详情", biz_type = Query)]
#[get_api("/job/{id}")]
pub async fn get_job(
    Component(svc): Component<JobService>,
    Path(id): Path<u64>,
) -> ApiResult<Json<RatchJobVo>> {
    let vo = svc.get_job(id).await?;
    Ok(Json(vo))
}

#[log(module = "任务管理", action = "创建任务", biz_type = Create)]
#[post_api("/job")]
pub async fn create_job(
    Component(svc): Component<JobService>,
    ValidatedJson(dto): ValidatedJson<CreateJobDto>,
) -> ApiResult<()> {
    svc.create_job(dto).await?;
    Ok(())
}

#[log(module = "任务管理", action = "更新任务", biz_type = Update)]
#[put_api("/job/{id}")]
pub async fn update_job(
    Component(svc): Component<JobService>,
    Path(id): Path<u64>,
    ValidatedJson(dto): ValidatedJson<UpdateJobDto>,
) -> ApiResult<()> {
    svc.update_job(id, dto).await?;
    Ok(())
}

#[log(module = "任务管理", action = "删除任务", biz_type = Delete)]
#[delete_api("/job/{id}")]
pub async fn remove_job(
    Component(svc): Component<JobService>,
    Path(id): Path<u64>,
) -> ApiResult<()> {
    svc.remove_job(id).await?;
    Ok(())
}

#[log(module = "任务管理", action = "触发任务", biz_type = Update)]
#[post_api("/job/{id}/trigger")]
pub async fn trigger_job(
    Component(svc): Component<JobService>,
    Path(id): Path<u64>,
    ValidatedJson(dto): ValidatedJson<TriggerJobDto>,
) -> ApiResult<()> {
    svc.trigger_job(id, dto).await?;
    Ok(())
}

#[log(module = "任务管理", action = "启用任务", biz_type = Update)]
#[put_api("/job/{id}/enable")]
pub async fn enable_job(
    Component(svc): Component<JobService>,
    Path(id): Path<u64>,
) -> ApiResult<()> {
    svc.enable_job(id).await?;
    Ok(())
}

#[log(module = "任务管理", action = "停用任务", biz_type = Update)]
#[put_api("/job/{id}/disable")]
pub async fn disable_job(
    Component(svc): Component<JobService>,
    Path(id): Path<u64>,
) -> ApiResult<()> {
    svc.disable_job(id).await?;
    Ok(())
}

#[log(module = "任务管理", action = "查询任务执行记录", biz_type = Query)]
#[get_api("/job/{id}/tasks")]
pub async fn list_job_tasks(
    Component(svc): Component<JobService>,
    Path(id): Path<u64>,
    Query(query): Query<JobTaskQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RatchJobTaskVo>>> {
    let vo = svc.task_list(id, query, pagination).await?;
    Ok(Json(vo))
}

#[log(module = "任务管理", action = "查询任务最新执行历史", biz_type = Query)]
#[get_api("/job/{id}/latest-history")]
pub async fn list_latest_history(
    Component(svc): Component<JobService>,
    Path(id): Path<u64>,
    Query(query): Query<JobTaskQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RatchJobTaskVo>>> {
    let vo = svc.latest_history(id, query, pagination).await?;
    Ok(Json(vo))
}

#[log(module = "任务管理", action = "根据Key查询任务ID", biz_type = Query)]
#[get_api("/job/query-id-by-key")]
pub async fn query_id_by_key(
    Component(svc): Component<JobService>,
    Query(query): Query<JobKeyQueryDto>,
) -> ApiResult<Json<u64>> {
    let id = svc.query_id_by_key(query).await?;
    Ok(Json(id))
}

#[log(module = "任务管理", action = "根据Key查询任务详情", biz_type = Query)]
#[get_api("/job/query-by-key")]
pub async fn query_job_by_key(
    Component(svc): Component<JobService>,
    Query(query): Query<JobKeyQueryDto>,
) -> ApiResult<Json<RatchJobVo>> {
    let vo = svc.query_job_by_key(query).await?;
    Ok(Json(vo))
}

#[log(module = "任务管理", action = "查询任务命名空间", biz_type = Query)]
#[get_api("/job/namespaces")]
pub async fn list_namespaces(
    Component(svc): Component<JobService>,
) -> ApiResult<Json<Vec<RatchNamespaceVo>>> {
    let vo = svc.list_namespaces().await?;
    Ok(Json(vo))
}

#[log(module = "任务管理", action = "查询任务应用列表", biz_type = Query)]
#[get_api("/job/apps")]
pub async fn list_apps(Component(svc): Component<JobService>) -> ApiResult<Json<Vec<String>>> {
    let vo = svc.list_apps().await?;
    Ok(Json(vo))
}
