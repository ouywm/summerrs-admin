use std::sync::Arc;

use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::dto::{
    BatchFailure, BatchResultVo, CreateJobDto, HandlerVo, JobDetailVo, JobQueryDto, JobRunQueryDto,
    JobRunVo, JobVo, UpdateJobDto,
};
use crate::engine::SchedulerHandle;
use crate::entity::{sys_job, sys_job_run};
use crate::enums::TriggerType;
use crate::registry::HandlerRegistry;

#[derive(Clone, Service)]
pub struct JobService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    registry: Arc<HandlerRegistry>,
    #[inject(component)]
    handle: SchedulerHandle,
}

impl JobService {
    // ---- handler 列表 ----

    pub fn list_handlers(&self) -> Vec<HandlerVo> {
        self.registry
            .names()
            .into_iter()
            .map(|name| HandlerVo {
                name: name.to_string(),
            })
            .collect()
    }

    pub fn handler_exists(&self, name: &str) -> bool {
        self.registry.contains(name)
    }

    // ---- 任务 CRUD ----

    pub async fn create_job(
        &self,
        dto: CreateJobDto,
        operator_id: Option<i64>,
    ) -> ApiResult<JobDetailVo> {
        if !self.handler_exists(&dto.handler) {
            return Err(ApiErrors::BadRequest(format!(
                "handler 不存在: {}（请在代码中用 #[job_handler(\"{}\")] 注册）",
                dto.handler, dto.handler
            )));
        }

        let existing = sys_job::Entity::find()
            .filter(sys_job::Column::Name.eq(&dto.name))
            .filter(match dto.tenant_id {
                Some(tid) => sys_job::Column::TenantId.eq(tid),
                None => sys_job::Column::TenantId.is_null(),
            })
            .one(&self.db)
            .await
            .context("查询任务重名失败")?;
        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!("任务名已存在: {}", dto.name)));
        }

        let active = dto.into_active_model(operator_id);
        let model = active.insert(&self.db).await.context("创建任务失败")?;

        self.sync_job_upserted(&model).await;
        Ok(JobDetailVo::from_model(model))
    }

    pub async fn update_job(&self, id: i64, dto: UpdateJobDto) -> ApiResult<JobDetailVo> {
        let job = sys_job::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询任务失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("任务不存在: {id}")))?;

        if let Some(new_handler) = dto.handler.as_deref()
            && !self.handler_exists(new_handler)
        {
            return Err(ApiErrors::BadRequest(format!(
                "handler 不存在: {new_handler}"
            )));
        }

        let current_version = job.version;
        let mut active: sys_job::ActiveModel = job.into();
        dto.apply_to(&mut active, current_version);
        let model = active.update(&self.db).await.context("更新任务失败")?;

        self.sync_job_upserted(&model).await;
        Ok(JobDetailVo::from_model(model))
    }

    pub async fn delete_job(&self, id: i64) -> ApiResult<()> {
        let res = sys_job::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除任务失败")?;
        if res.rows_affected == 0 {
            return Err(ApiErrors::NotFound(format!("任务不存在: {id}")));
        }
        self.sync_job_removed(id).await;
        Ok(())
    }

    pub async fn toggle_enabled(&self, id: i64, enabled: bool) -> ApiResult<JobDetailVo> {
        let job = sys_job::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询任务失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("任务不存在: {id}")))?;
        if job.enabled == enabled {
            return Ok(JobDetailVo::from_model(job));
        }
        let next_version = job.version + 1;
        let mut active: sys_job::ActiveModel = job.into();
        active.enabled = Set(enabled);
        active.version = Set(next_version);
        let model = active.update(&self.db).await.context("切换任务状态失败")?;

        self.sync_job_upserted(&model).await;
        Ok(JobDetailVo::from_model(model))
    }

    /// 手动触发：在本实例异步执行，接口立即返回。
    pub async fn trigger_job(
        &self,
        id: i64,
        trigger_by: Option<i64>,
        params_override: Option<serde_json::Value>,
    ) -> ApiResult<()> {
        let job = sys_job::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询任务失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("任务不存在: {id}")))?;
        let Some(scheduler) = self.handle.current().await else {
            tracing::warn!(
                job_id = id,
                "scheduler not installed; manual trigger skipped"
            );
            return Ok(());
        };
        tokio::spawn(async move {
            scheduler
                .trigger_now(&job, TriggerType::Manual, trigger_by, params_override)
                .await;
        });
        Ok(())
    }

    pub async fn get_job_detail(&self, id: i64) -> ApiResult<JobDetailVo> {
        let model = sys_job::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询任务失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("任务不存在: {id}")))?;
        let last_runs = crate::engine::fetch_last_runs(&self.db, &[id])
            .await
            .context("查询最近执行记录失败")?;
        let last = last_runs.get(&id);
        let next_fire_at = crate::engine::compute_next_fire(
            &model,
            last.map(|l| l.scheduled_at),
            last.and_then(|l| l.finished_at),
            last.map(|l| l.state),
        );
        Ok(JobDetailVo::from_model_with_runtime(
            model,
            next_fire_at,
            last.map(|l| l.state),
            last.and_then(|l| l.finished_at),
        ))
    }

    pub async fn list_jobs(
        &self,
        query: JobQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<JobVo>> {
        let page = sys_job::Entity::find()
            .filter(query)
            .order_by_desc(sys_job::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("分页查询任务失败")?;

        // 批量取最近一次 run，给每个 job 算 nextFireAt + lastRun
        let ids: Vec<i64> = page.content.iter().map(|j| j.id).collect();
        let last_runs = crate::engine::fetch_last_runs(&self.db, &ids)
            .await
            .context("批量查询最近执行记录失败")?;

        Ok(page.map(|m| {
            let last = last_runs.get(&m.id);
            let next_fire_at = crate::engine::compute_next_fire(
                &m,
                last.map(|l| l.scheduled_at),
                last.and_then(|l| l.finished_at),
                last.map(|l| l.state),
            );
            JobVo::from_model_with_runtime(
                m,
                next_fire_at,
                last.map(|l| l.state),
                last.and_then(|l| l.finished_at),
            )
        }))
    }

    pub async fn list_all_enabled(&self) -> ApiResult<Vec<sys_job::Model>> {
        sys_job::Entity::find()
            .filter(sys_job::Column::Enabled.eq(true))
            .all(&self.db)
            .await
            .context("查询启用任务失败")
            .map_err(ApiErrors::Internal)
    }

    pub async fn import_builtin_if_absent(&self, dto: CreateJobDto) -> ApiResult<()> {
        let existing = sys_job::Entity::find()
            .filter(sys_job::Column::Name.eq(&dto.name))
            .filter(match dto.tenant_id {
                Some(tid) => sys_job::Column::TenantId.eq(tid),
                None => sys_job::Column::TenantId.is_null(),
            })
            .one(&self.db)
            .await
            .context("查询内置任务失败")?;
        if existing.is_some() {
            return Ok(());
        }
        if !self.handler_exists(&dto.handler) {
            return Err(ApiErrors::BadRequest(format!(
                "内置任务 handler 未注册: {}",
                dto.handler
            )));
        }
        let active = dto.into_active_model(None);
        active
            .insert(&self.db)
            .await
            .context("import 内置任务失败")?;
        Ok(())
    }

    async fn sync_job_upserted(&self, model: &sys_job::Model) {
        let Some(scheduler) = self.handle.current().await else {
            tracing::warn!(
                job_id = model.id,
                "scheduler not installed; runtime sync skipped"
            );
            return;
        };

        if model.enabled {
            if let Err(error) = scheduler.register_job(model).await {
                tracing::error!(
                    ?error,
                    job_id = model.id,
                    "register_job failed after job upsert"
                );
                return;
            }
            match crate::engine::evaluate_misfire(&self.db, model).await {
                Ok(decision) if decision.should_fire => {
                    tracing::info!(
                        job_id = model.id,
                        missed = decision.missed_count,
                        "job upsert: misfire FIRE_NOW catch-up"
                    );
                    let scheduler = scheduler.clone();
                    let model = model.clone();
                    tokio::spawn(async move {
                        scheduler.fire_misfire(&model).await;
                    });
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(?error, job_id = model.id, "misfire evaluate failed");
                }
            }
        } else {
            scheduler.remove_job(model.id).await;
        }
    }

    async fn sync_job_removed(&self, id: i64) {
        let Some(scheduler) = self.handle.current().await else {
            tracing::warn!(job_id = id, "scheduler not installed; remove sync skipped");
            return;
        };
        scheduler.remove_job(id).await;
    }

    // ---- 执行记录查询 ----

    pub async fn list_runs(
        &self,
        query: JobRunQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<JobRunVo>> {
        let page = sys_job_run::Entity::find()
            .filter(query)
            .order_by_desc(sys_job_run::Column::ScheduledAt)
            .page(&self.db, &pagination)
            .await
            .context("分页查询执行记录失败")?;
        Ok(page.map(JobRunVo::from_model))
    }

    pub async fn get_run_detail(&self, id: i64) -> ApiResult<JobRunVo> {
        let model = sys_job_run::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询执行记录失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("执行记录不存在: {id}")))?;
        Ok(JobRunVo::from_model(model))
    }

    // -----------------------------------------------------------------------
    // 批量操作（部分成功也算 200，前端按 failures 处理）
    // -----------------------------------------------------------------------

    pub async fn batch_toggle(&self, ids: Vec<i64>, enabled: bool) -> BatchResultVo {
        let mut failures = Vec::new();
        let mut success = 0;
        for id in ids {
            match self.toggle_enabled(id, enabled).await {
                Ok(_) => success += 1,
                Err(e) => failures.push(BatchFailure {
                    id,
                    reason: e.to_string(),
                }),
            }
        }
        BatchResultVo {
            success_count: success,
            failed_count: failures.len(),
            failures,
        }
    }

    pub async fn batch_delete(&self, ids: Vec<i64>) -> BatchResultVo {
        let mut failures = Vec::new();
        let mut success = 0;
        for id in ids {
            match self.delete_job(id).await {
                Ok(_) => success += 1,
                Err(e) => failures.push(BatchFailure {
                    id,
                    reason: e.to_string(),
                }),
            }
        }
        BatchResultVo {
            success_count: success,
            failed_count: failures.len(),
            failures,
        }
    }

    pub async fn batch_trigger(&self, ids: Vec<i64>, trigger_by: Option<i64>) -> BatchResultVo {
        let mut failures = Vec::new();
        let mut success = 0;
        for id in ids {
            match self.trigger_job(id, trigger_by, None).await {
                Ok(_) => success += 1,
                Err(e) => failures.push(BatchFailure {
                    id,
                    reason: e.to_string(),
                }),
            }
        }
        BatchResultVo {
            success_count: success,
            failed_count: failures.len(),
            failures,
        }
    }
}
