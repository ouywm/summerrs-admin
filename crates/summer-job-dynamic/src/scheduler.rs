//! 单机调度器 —— DB ↔ tokio-cron-scheduler 同步 + 任务执行 + 执行记录落库。
//!
//! 合并原 engine/{scheduler, worker, handle} 三个模块。单机模式，没有 blocking
//! 策略、misfire 补跑、unique 去重、依赖触发这些多机时代的复杂性。
//!
//! 执行流程：
//! 1. 上次执行还没结束 → 直接写 `DISCARDED`（简化版 DISCARD，没有排队/取消选项）
//! 2. INSERT `sys_job_run` 拿 run_id，state = RUNNING
//! 3. 从 registry 查 handler
//! 4. `tokio::time::timeout` 包 handler + `catch_unwind` 兜底 panic
//! 5. 更新 run 终态：SUCCEEDED / FAILED / TIMEOUT
//! 6. 失败 & retry_count < retry_max → 同 task 内 sleep 指数退避后重跑

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use chrono::NaiveDateTime;
use sea_orm::{ActiveModelTrait, NotSet, Set};
use serde_json::Value;
use summer::app::App;
use summer_job::JobScheduler;
use summer_sea_orm::DbConn;
use tokio::sync::RwLock;
use tokio_cron_scheduler::Job as CronJob;
use uuid::Uuid;

use crate::context::{JobContext, JobError};
use crate::entity::{sys_job, sys_job_run};
use crate::enums::{RunState, ScheduleType, TriggerType};
use crate::registry::HandlerRegistry;

/// 指数退避的基础延迟。第 n 次重试延迟 = BASE * 2^n，最多 10 分钟。
const RETRY_BASE_SECS: u64 = 5;
const RETRY_MAX_SECS: u64 = 600;

/// 机器实例标识，写 sys_job_run.instance 用（单机模式纯留痕，不做领导选举）。
pub fn instance_id() -> String {
    let host = sysinfo::System::host_name().unwrap_or_else(|| "unknown".to_string());
    format!("{}:{}", host, std::process::id())
}

// ---------------------------------------------------------------------------
// SchedulerHandle —— 延迟安装的 scheduler 句柄
// ---------------------------------------------------------------------------

/// Plugin build 阶段先 register 一个空 Handle 到 ComponentRegistry，
/// scheduler 真正 start 后再 install。JobService 的 CRUD 通过这个 handle
/// 触发 register/remove，避免 JobService ↔ Scheduler 的循环依赖。
#[derive(Clone, Default)]
pub struct SchedulerHandle(Arc<RwLock<Option<DynamicScheduler>>>);

impl SchedulerHandle {
    pub async fn install(&self, scheduler: DynamicScheduler) {
        *self.0.write().await = Some(scheduler);
    }

    pub async fn current(&self) -> Option<DynamicScheduler> {
        self.0.read().await.clone()
    }
}

// ---------------------------------------------------------------------------
// DynamicScheduler —— DB ↔ tokio-cron-scheduler 同步
// ---------------------------------------------------------------------------

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
        let uuid = self
            .cron_sched
            .clone()
            .add(cron_job)
            .await
            .context("scheduler.add(job) failed")?;
        self.job_uuids.write().await.insert(model.id, uuid);
        tracing::info!(
            job_id = model.id,
            name = %model.name,
            handler = %model.handler,
            schedule_type = ?model.schedule_type,
            uuid = %uuid,
            "job registered to scheduler"
        );
        Ok(())
    }

    /// 从 scheduler 移除任务（best-effort，不存在时静默成功）。
    pub async fn remove_job(&self, job_id: i64) {
        let uuid = self.job_uuids.write().await.remove(&job_id);
        if let Some(uuid) = uuid
            && let Err(error) = self.cron_sched.clone().remove(&uuid).await
        {
            tracing::warn!(job_id, %uuid, ?error, "scheduler.remove(uuid) failed");
        }
    }

    /// 直接执行（手动触发 / API 触发，不走 cron tick）。
    pub async fn trigger_now(
        &self,
        job: &sys_job::Model,
        trigger_type: TriggerType,
        trigger_by: Option<i64>,
        params_override: Option<Value>,
    ) {
        let now = chrono::Local::now().naive_local();
        self.worker
            .execute(job, trigger_type, trigger_by, params_override, now)
            .await;
    }

    /// 返回当前已注册的任务数。
    pub async fn registered_count(&self) -> usize {
        self.job_uuids.read().await.len()
    }

    fn build_cron_job(&self, model: &sys_job::Model) -> anyhow::Result<CronJob> {
        let worker = self.worker.clone();
        let model_owned = model.clone();

        let runner = move |_uuid: Uuid, _sched: tokio_cron_scheduler::JobScheduler| {
            let worker = worker.clone();
            let m = model_owned.clone();
            let now = chrono::Local::now().naive_local();
            Box::pin(async move {
                worker.execute(&m, TriggerType::Cron, None, None, now).await;
            }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        };

        let job = match model.schedule_type {
            ScheduleType::Cron => {
                let expr = model
                    .cron_expr
                    .as_deref()
                    .context("schedule_type=CRON 时 cron_expr 必填")?;
                CronJob::new_async_tz(expr, chrono::Local, runner).context("构造 cron job 失败")?
            }
            ScheduleType::FixedRate => {
                let interval_ms = model
                    .interval_ms
                    .filter(|v| *v > 0)
                    .context("schedule_type=FIXED_RATE 时 interval_ms 必填且大于 0")?;
                CronJob::new_repeated_async(Duration::from_millis(interval_ms as u64), runner)
                    .context("构造 repeated job 失败")?
            }
            ScheduleType::Oneshot => {
                let fire = model
                    .fire_time
                    .context("schedule_type=ONESHOT 时 fire_time 必填")?;
                let now = chrono::Local::now().naive_local();
                let delay = (fire - now).num_milliseconds().max(0) as u64;
                CronJob::new_one_shot_async(Duration::from_millis(delay), runner)
                    .context("构造 one-shot job 失败")?
            }
        };

        Ok(job)
    }
}

// ---------------------------------------------------------------------------
// Worker —— 单次任务执行
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Worker {
    pub db: DbConn,
    pub registry: Arc<HandlerRegistry>,
    pub app: Arc<App>,
    /// 正在执行的 job_id 集合（简化版 DISCARD：在集合里就跳过本次触发）
    in_flight: Arc<RwLock<std::collections::HashSet<i64>>>,
}

impl Worker {
    pub fn new(db: DbConn, registry: Arc<HandlerRegistry>, app: Arc<App>) -> Self {
        Self {
            db,
            registry,
            app,
            in_flight: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    /// 执行一次触发，失败时按 `retry_max` + 指数退避在同 task 内 loop 重试。
    pub async fn execute(
        &self,
        job: &sys_job::Model,
        initial_trigger_type: TriggerType,
        trigger_by: Option<i64>,
        initial_params_override: Option<Value>,
        initial_scheduled_at: NaiveDateTime,
    ) {
        // 简化版 DISCARD：上次还在跑就跳过
        {
            let mut set = self.in_flight.write().await;
            if set.contains(&job.id) {
                tracing::info!(
                    job_id = job.id,
                    "previous run still in flight; recording DISCARDED"
                );
                drop(set);
                self.record_discarded(
                    job,
                    initial_trigger_type,
                    trigger_by,
                    initial_scheduled_at,
                    0,
                )
                .await;
                return;
            }
            set.insert(job.id);
        }

        let mut trigger_type = initial_trigger_type;
        let params_override = initial_params_override;
        let mut scheduled_at = initial_scheduled_at;
        let mut retry_count: u32 = 0;

        loop {
            let outcome = self
                .run_once(
                    job,
                    trigger_type,
                    trigger_by,
                    params_override.clone(),
                    scheduled_at,
                    retry_count,
                )
                .await;

            if outcome.should_retry() && retry_count < job.retry_max.max(0) as u32 {
                let delay_secs = (RETRY_BASE_SECS << retry_count.min(10)).min(RETRY_MAX_SECS);
                tracing::info!(
                    job_id = job.id,
                    next_retry = retry_count + 1,
                    retry_max = job.retry_max,
                    delay_secs,
                    "scheduling retry"
                );
                tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                retry_count += 1;
                trigger_type = TriggerType::Retry;
                scheduled_at = chrono::Local::now().naive_local();
                continue;
            }
            break;
        }

        self.in_flight.write().await.remove(&job.id);
    }

    async fn run_once(
        &self,
        job: &sys_job::Model,
        trigger_type: TriggerType,
        trigger_by: Option<i64>,
        params_override: Option<Value>,
        scheduled_at: NaiveDateTime,
        retry_count: u32,
    ) -> RunOutcome {
        let trace_id = Uuid::new_v4().to_string();
        let now = chrono::Local::now().naive_local();
        let params = params_override.unwrap_or_else(|| job.params_json.clone());

        let run_id = match self
            .insert_run(
                job,
                &trace_id,
                trigger_type,
                trigger_by,
                scheduled_at,
                now,
                retry_count,
                RunState::Running,
            )
            .await
        {
            Ok(id) => id,
            Err(error) => {
                tracing::error!(
                    job_id = job.id,
                    handler = %job.handler,
                    ?error,
                    "failed to create job_run record; skipping execution"
                );
                return RunOutcome::CreateRunFailed;
            }
        };

        let Some(handler_fn) = self.registry.get(&job.handler) else {
            self.finalize(
                run_id,
                RunState::Failed,
                None,
                Some(format!("handler 未注册: {}", job.handler)),
            )
            .await;
            return RunOutcome::Failed;
        };

        let ctx = JobContext {
            run_id,
            job_id: job.id,
            trace_id,
            params,
            retry_count: retry_count as i32,
            app: self.app.clone(),
        };

        let timeout = (job.timeout_ms > 0).then(|| Duration::from_millis(job.timeout_ms as u64));

        use futures::FutureExt;
        let fut = std::panic::AssertUnwindSafe(handler_fn(ctx));
        let outcome = match timeout {
            Some(d) => match tokio::time::timeout(d, fut.catch_unwind()).await {
                Ok(Ok(res)) => res.map_err(HandlerOutcome::from_handler_error),
                Ok(Err(panic_payload)) => Err(HandlerOutcome::Failed(format_panic(panic_payload))),
                Err(_) => Err(HandlerOutcome::Timeout(d)),
            },
            None => match fut.catch_unwind().await {
                Ok(res) => res.map_err(HandlerOutcome::from_handler_error),
                Err(panic_payload) => Err(HandlerOutcome::Failed(format_panic(panic_payload))),
            },
        };

        match outcome {
            Ok(value) => {
                self.finalize(run_id, RunState::Succeeded, Some(value), None)
                    .await;
                RunOutcome::Succeeded
            }
            Err(HandlerOutcome::Timeout(d)) => {
                self.finalize(
                    run_id,
                    RunState::Timeout,
                    None,
                    Some(format!("handler timeout after {d:?}")),
                )
                .await;
                RunOutcome::Timeout
            }
            Err(HandlerOutcome::Failed(msg)) => {
                self.finalize(run_id, RunState::Failed, None, Some(msg))
                    .await;
                RunOutcome::Failed
            }
        }
    }

    async fn record_discarded(
        &self,
        job: &sys_job::Model,
        trigger_type: TriggerType,
        trigger_by: Option<i64>,
        scheduled_at: NaiveDateTime,
        retry_count: u32,
    ) {
        let trace_id = Uuid::new_v4().to_string();
        let now = chrono::Local::now().naive_local();
        let active = sys_job_run::ActiveModel {
            id: NotSet,
            job_id: Set(job.id),
            trace_id: Set(trace_id),
            trigger_type: Set(trigger_type),
            trigger_by: Set(trigger_by),
            state: Set(RunState::Discarded),
            scheduled_at: Set(scheduled_at),
            started_at: Set(None),
            finished_at: Set(Some(now)),
            retry_count: Set(retry_count as i32),
            result_json: Set(None),
            error_message: Set(Some("blocked by previous run".into())),
            create_time: NotSet,
        };
        if let Err(error) = active.insert(&self.db).await {
            tracing::error!(?error, job_id = job.id, "record DISCARDED state failed");
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_run(
        &self,
        job: &sys_job::Model,
        trace_id: &str,
        trigger_type: TriggerType,
        trigger_by: Option<i64>,
        scheduled_at: NaiveDateTime,
        started_at: NaiveDateTime,
        retry_count: u32,
        state: RunState,
    ) -> anyhow::Result<i64> {
        let active = sys_job_run::ActiveModel {
            id: NotSet,
            job_id: Set(job.id),
            trace_id: Set(trace_id.to_string()),
            trigger_type: Set(trigger_type),
            trigger_by: Set(trigger_by),
            state: Set(state),
            scheduled_at: Set(scheduled_at),
            started_at: Set(Some(started_at)),
            finished_at: Set(None),
            retry_count: Set(retry_count as i32),
            result_json: Set(None),
            error_message: Set(None),
            create_time: NotSet,
        };
        let model = active.insert(&self.db).await.context("插入 job_run 失败")?;
        Ok(model.id)
    }

    async fn finalize(
        &self,
        run_id: i64,
        state: RunState,
        result_json: Option<Value>,
        error_message: Option<String>,
    ) {
        let now = chrono::Local::now().naive_local();
        let active = sys_job_run::ActiveModel {
            id: Set(run_id),
            state: Set(state),
            finished_at: Set(Some(now)),
            result_json: Set(result_json),
            error_message: Set(error_message),
            ..Default::default()
        };
        if let Err(error) = active.update(&self.db).await {
            tracing::error!(
                run_id,
                ?error,
                "failed to finalize job_run; record may be left in RUNNING state"
            );
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RunOutcome {
    Succeeded,
    Failed,
    Timeout,
    /// 连 sys_job_run 都没创建成功（DB 故障）—— 不重试
    CreateRunFailed,
}

impl RunOutcome {
    fn should_retry(&self) -> bool {
        matches!(self, Self::Failed | Self::Timeout)
    }
}

enum HandlerOutcome {
    Timeout(Duration),
    Failed(String),
}

impl HandlerOutcome {
    fn from_handler_error(err: JobError) -> Self {
        match err {
            JobError::Timeout(d) => Self::Timeout(d),
            other => Self::Failed(other.to_string()),
        }
    }
}

fn format_panic(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        format!("handler panicked: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("handler panicked: {s}")
    } else {
        "handler panicked (unknown payload)".to_string()
    }
}
