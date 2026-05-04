//! DB ↔ tokio-cron-scheduler 的同步层。
//!
//! 复用 `summer_job::JobPlugin` 已注册的 `JobScheduler` component，把 DB 里 enabled
//! 的 `sys_job` 注册为 cron / repeated / one-shot 任务。
//!
//! 任务触发时通过 `Worker::execute` 调用 handler 并写 `sys_job_run`。
//!
//! 维护 `job_id → tokio Job uuid` 映射，admin 改动后通过 `register / remove / reload`
//! 同步到 scheduler 实例。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use summer_job::JobScheduler;
use tokio::sync::RwLock;
use tokio_cron_scheduler::Job as CronJob;
use uuid::Uuid;

use crate::engine::worker::Worker;
use crate::entity::sys_job;
use crate::enums::{ScheduleType, TriggerType};

#[derive(Clone)]
pub struct DynamicScheduler {
    cron_sched: JobScheduler,
    worker: Arc<Worker>,
    job_uuids: Arc<RwLock<HashMap<i64, Uuid>>>,
}

impl DynamicScheduler {
    pub fn new(cron_sched: JobScheduler, worker: Worker) -> Self {
        Self {
            cron_sched,
            worker: Arc::new(worker),
            job_uuids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 启动期一次性把所有 enabled job 注册到 scheduler。
    pub async fn load_and_register_all(&self, jobs: Vec<sys_job::Model>) -> anyhow::Result<usize> {
        let mut ok = 0usize;
        for job in jobs {
            match self.register_job(&job).await {
                Ok(_) => ok += 1,
                Err(error) => {
                    tracing::error!(
                        job_id = job.id,
                        name = %job.name,
                        ?error,
                        "failed to register job; skipping"
                    );
                }
            }
        }
        Ok(ok)
    }

    /// 把单个任务注册到 scheduler；若已存在先 remove 再重建。
    pub async fn register_job(&self, model: &sys_job::Model) -> anyhow::Result<()> {
        self.remove_job(model.id).await;

        let cron_job = self.build_cron_job(model)?;
        let sched = self.cron_sched.clone();
        let uuid = sched
            .add(cron_job)
            .await
            .context("scheduler.add(job) failed")?;
        self.job_uuids.write().await.insert(model.id, uuid);
        tracing::info!(
            job_id = model.id,
            name = %model.name,
            handler = %model.handler,
            schedule_type = ?model.schedule_type,
            cron = ?model.cron_expr,
            interval_ms = ?model.interval_ms,
            uuid = %uuid,
            "job registered to scheduler"
        );
        Ok(())
    }

    /// 从 scheduler 移除任务（best-effort，不存在时静默成功）。
    pub async fn remove_job(&self, job_id: i64) {
        let uuid = self.job_uuids.write().await.remove(&job_id);
        if let Some(uuid) = uuid {
            let sched = self.cron_sched.clone();
            if let Err(error) = sched.remove(&uuid).await {
                tracing::warn!(job_id, %uuid, ?error, "scheduler.remove(uuid) failed");
            }
        }
    }

    /// 移除当前实例上由本 scheduler 注册的所有 job。
    pub async fn unregister_all_managed(&self) {
        let ids: Vec<i64> = {
            let map = self.job_uuids.read().await;
            map.keys().copied().collect()
        };
        for id in ids {
            self.remove_job(id).await;
        }
        tracing::info!(
            "unregistered all managed jobs from local scheduler (role downgraded to follower)"
        );
    }

    /// 直接执行（手动触发 / 依赖触发 / API 触发，不走 cron tick）。
    /// `trigger_type` 决定写入 sys_job_run 的来源标签，便于审计区分。
    pub async fn trigger_now(
        &self,
        job: &sys_job::Model,
        trigger_type: TriggerType,
        trigger_by: Option<i64>,
        params_override: Option<serde_json::Value>,
    ) {
        let now = chrono::Local::now().naive_local();
        self.worker
            .execute(job, trigger_type, trigger_by, params_override, now)
            .await;
    }

    /// Misfire 补跑：trigger_type 标记为 Misfire 以便审计。
    pub async fn fire_misfire(&self, job: &sys_job::Model) {
        let now = chrono::Local::now().naive_local();
        self.worker
            .execute(job, TriggerType::Misfire, None, None, now)
            .await;
    }

    /// 返回当前已注册的任务数（仅本实例视角）。
    pub async fn registered_count(&self) -> usize {
        self.job_uuids.read().await.len()
    }

    /// 根据 `schedule_type` 构造对应类型的 tokio job。闭包内 spawn worker。
    fn build_cron_job(&self, model: &sys_job::Model) -> anyhow::Result<CronJob> {
        let worker = self.worker.clone();
        let model_for_closure = model.clone();

        let make_runner = move |worker: Arc<Worker>, m: sys_job::Model| {
            move |_uuid: Uuid, _sched: tokio_cron_scheduler::JobScheduler| {
                let worker = worker.clone();
                let m = m.clone();
                let now = chrono::Local::now().naive_local();
                Box::pin(async move {
                    worker.execute(&m, TriggerType::Cron, None, None, now).await;
                })
                    as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            }
        };

        let job = match model.schedule_type {
            ScheduleType::Cron => {
                let expr = model
                    .cron_expr
                    .as_deref()
                    .context("schedule_type=CRON 时 cron_expr 必填")?;
                CronJob::new_async_tz(expr, chrono::Local, make_runner(worker, model_for_closure))
                    .context("构造 cron job 失败")?
            }
            ScheduleType::FixedRate | ScheduleType::FixedDelay => {
                // tokio-cron-scheduler 0.15 的 FixedDelay / FixedRate 内核相同，差异在
                // 实现层未完全分化（参考 issue #56），先以 repeated 提供一致语义。
                let interval_ms = model
                    .interval_ms
                    .filter(|v| *v > 0)
                    .context("schedule_type=FIXED_* 时 interval_ms 必填且大于 0")?;
                CronJob::new_repeated_async(
                    Duration::from_millis(interval_ms as u64),
                    make_runner(worker, model_for_closure),
                )
                .context("构造 repeated job 失败")?
            }
            ScheduleType::Oneshot => {
                let fire = model
                    .fire_time
                    .context("schedule_type=ONESHOT 时 fire_time 必填")?;
                let now = chrono::Local::now().naive_local();
                let delay = (fire - now).num_milliseconds().max(0) as u64;
                CronJob::new_one_shot_async(
                    Duration::from_millis(delay),
                    make_runner(worker, model_for_closure),
                )
                .context("构造 one-shot job 失败")?
            }
        };

        Ok(job)
    }
}
