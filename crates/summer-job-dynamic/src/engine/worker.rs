//! 单次任务执行单元 —— 由 scheduler tick / 手动触发 / 重试调度共同使用。
//!
//! 流程：
//! 1. 按 `model.blocking` 抢 RunningTracker slot；Discard 时直接写 DISCARDED 退出
//! 2. INSERT `sys_job_run` 拿到 run_id（状态 RUNNING）
//! 3. 从 registry 找 handler
//! 4. `tokio::time::timeout` 包 handler 调用，CancellationToken 配合 cooperative cancel
//! 5. 根据返回结果更新 run 终态：SUCCEEDED / FAILED / TIMEOUT / CANCELED
//! 6. 失败 & retry_count < retry_max：spawn 延时任务重试（trigger_type = Retry）

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use chrono::NaiveDateTime;
use sea_orm::{ActiveModelTrait, NotSet, Set};
use serde_json::Value;
use summer::app::App;
use summer_sea_orm::DbConn;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::context::{JobContext, JobError};
use crate::engine::blocking::{AcquireResult, BlockingGuard, BlockingTracker};
use crate::engine::local_trigger::{LocalTrigger, LocalTriggerSender};
use crate::engine::retry::next_retry_delay;
use crate::entity::{sys_job, sys_job_run};
use crate::enums::{DependencyOnState, RunState, TriggerType};
use crate::registry::HandlerRegistry;
use crate::service::DependencyService;

pub fn instance_id() -> String {
    let host = sysinfo::System::host_name().unwrap_or_else(|| "unknown".to_string());
    format!("{}:{}", host, std::process::id())
}

#[derive(Clone)]
pub struct Worker {
    pub db: DbConn,
    pub registry: Arc<HandlerRegistry>,
    pub app: Arc<App>,
    pub instance: Arc<str>,
    pub tracker: BlockingTracker,
    pub metrics: Arc<crate::engine::SchedulerMetrics>,
    pub trigger_tx: LocalTriggerSender,
    pub dep_service: DependencyService,
}

impl Worker {
    pub fn new(
        db: DbConn,
        registry: Arc<HandlerRegistry>,
        app: Arc<App>,
        metrics: Arc<crate::engine::SchedulerMetrics>,
        trigger_tx: LocalTriggerSender,
        dep_service: DependencyService,
    ) -> Self {
        Self {
            db,
            registry,
            app,
            instance: Arc::from(instance_id()),
            tracker: BlockingTracker::default(),
            metrics,
            trigger_tx,
            dep_service,
        }
    }

    /// 执行一次任务触发。失败时按 `retry_max` + `retry_backoff` 在同一 task 内 loop 重试。
    ///
    /// 用 loop 而非递归 spawn，避免 async fn 自递归触发的 `Send` 推断问题；
    /// 重试本身也只是一个 task 内的延时 + 再次执行，跟 spawn 新 task 等价。
    pub async fn execute(
        &self,
        job: &sys_job::Model,
        initial_trigger_type: TriggerType,
        trigger_by: Option<i64>,
        initial_params_override: Option<Value>,
        initial_scheduled_at: NaiveDateTime,
    ) {
        let mut trigger_type = initial_trigger_type;
        let params_override = initial_params_override;
        let mut scheduled_at = initial_scheduled_at;
        let mut retry_count: u32 = 0;

        loop {
            // 按 model.blocking 抢执行权：Discard 失败 → 写 DISCARDED 退出；Serial → await 排队；Override → 立即抢占
            let guard: BlockingGuard = match self.tracker.acquire(job.id, job.blocking).await {
                AcquireResult::Acquired(g) => g,
                AcquireResult::Discarded => {
                    tracing::info!(
                        job_id = job.id,
                        blocking = ?job.blocking,
                        "discard policy: previous run still in flight; recording DISCARDED"
                    );
                    self.record_discarded(job, trigger_type, trigger_by, scheduled_at, retry_count)
                        .await;
                    return;
                }
            };
            let outer_cancel = guard.outer_cancel();

            let outcome = self
                .run_once(
                    job,
                    trigger_type,
                    trigger_by,
                    params_override.clone(),
                    scheduled_at,
                    retry_count,
                    outer_cancel,
                )
                .await;
            drop(guard);

            if outcome.should_retry() && retry_count < job.retry_max.max(0) as u32 {
                let delay = next_retry_delay(retry_count, job.retry_backoff);
                tracing::info!(
                    job_id = job.id,
                    next_retry = retry_count + 1,
                    retry_max = job.retry_max,
                    delay_secs = delay.as_secs(),
                    "scheduling retry"
                );
                tokio::time::sleep(delay).await;
                retry_count += 1;
                trigger_type = TriggerType::Retry;
                scheduled_at = chrono::Local::now().naive_local();
                continue;
            }
            return;
        }
    }

    /// 单次执行（不含重试 / 不含 blocking 抢占）。
    ///
    /// `outer_cancel` 由 Override 阻塞策略提供：被新 trigger 抢占时该 token 会被 cancel，
    /// worker 内部 spawn 一个 watcher 把信号传到 `ctx.cancel`，handler 通过
    /// [`crate::JobContext::check_cancel`] cooperative 退出。其他策略下传 `None`。
    #[allow(clippy::too_many_arguments)]
    async fn run_once(
        &self,
        job: &sys_job::Model,
        trigger_type: TriggerType,
        trigger_by: Option<i64>,
        params_override: Option<Value>,
        scheduled_at: NaiveDateTime,
        retry_count: u32,
        outer_cancel: Option<CancellationToken>,
    ) -> RunOutcome {
        // metrics: 触发分桶 + in_flight +1（finalize 时 -1，保证配对）
        self.metrics.record_trigger(trigger_type);
        self.metrics.inc_in_flight();
        let trace_id = Uuid::new_v4().to_string();
        let now = chrono::Local::now().naive_local();

        let params = params_override.unwrap_or_else(|| job.params_json.clone());

        // Unique 去重：仅 Cron / Manual / Misfire / Api 参与（Retry / Workflow 跳过）
        let unique_lock = if crate::engine::unique::should_apply(trigger_type) {
            crate::engine::unique::compute_lock_value(job, &params)
        } else {
            None
        };
        if let Some(lock) = unique_lock.as_ref()
            && crate::engine::unique::has_conflict(&self.db, job.id, lock).await
        {
            tracing::info!(
                job_id = job.id,
                ?trigger_type,
                "unique conflict: previous run with same key still active; skipping"
            );
            self.metrics.dec_in_flight();
            self.record_unique_skipped(
                job,
                trigger_type,
                trigger_by,
                scheduled_at,
                retry_count,
                lock.clone(),
            )
            .await;
            return RunOutcome::UniqueSkipped;
        }

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
                unique_lock.clone(),
            )
            .await
        {
            Ok(id) => id,
            Err(error) => {
                tracing::error!(
                    job_id = job.id,
                    handler = %job.handler,
                    error = ?error,
                    "failed to create job_run record; skipping execution"
                );
                self.metrics.dec_in_flight();
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

        let cancel = CancellationToken::new();
        // Override 桥接：outer_cancel 被 cancel 时同步 cancel 内部 token
        if let Some(outer) = outer_cancel {
            let inner = cancel.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = outer.cancelled() => inner.cancel(),
                    _ = inner.cancelled() => (),  // 内部已结束，watcher 退出
                }
            });
        }
        let ctx = JobContext {
            run_id,
            job_id: job.id,
            trace_id,
            params,
            retry_count: retry_count as i32,
            cancel: cancel.clone(),
            app: self.app.clone(),
            script: job.script.clone(),
        };

        let timeout = if job.timeout_ms > 0 {
            Some(Duration::from_millis(job.timeout_ms as u64))
        } else {
            None
        };

        let fut = std::panic::AssertUnwindSafe(handler_fn(ctx));
        use futures::FutureExt;
        let outcome = match timeout {
            Some(d) => match tokio::time::timeout(d, fut.catch_unwind()).await {
                Ok(Ok(res)) => res.map_err(HandlerOutcome::from_handler_error),
                Ok(Err(panic_payload)) => Err(HandlerOutcome::Failed(format_panic(panic_payload))),
                Err(_) => {
                    cancel.cancel();
                    Err(HandlerOutcome::Timeout(d))
                }
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
                self.try_fire_downstream(job, run_id, DependencyOnState::Succeeded)
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
                self.try_fire_downstream(job, run_id, DependencyOnState::Always)
                    .await;
                RunOutcome::Timeout
            }
            Err(HandlerOutcome::Canceled) => {
                self.finalize(run_id, RunState::Canceled, None, Some("canceled".into()))
                    .await;
                self.try_fire_downstream(job, run_id, DependencyOnState::Always)
                    .await;
                RunOutcome::Canceled
            }
            Err(HandlerOutcome::Failed(msg)) => {
                self.finalize(run_id, RunState::Failed, None, Some(msg))
                    .await;
                self.try_fire_downstream(job, run_id, DependencyOnState::Failed)
                    .await;
                RunOutcome::Failed
            }
        }
    }

    /// 上游执行结束后按 `on_state` 评估并触发下游 job。
    /// 单实例模式下直接在本进程触发，仍复用 `execute` 保持 blocking / metrics 一致。
    /// 失败仅 log，不影响主链路；重试链路也不会重复触发（仅终态成功才走 SUCCEEDED 分支）。
    async fn try_fire_downstream(
        &self,
        upstream: &sys_job::Model,
        upstream_run_id: i64,
        terminal: DependencyOnState,
    ) {
        let downstreams = self.dep_service.list_to_fire(upstream.id, terminal).await;
        if downstreams.is_empty() {
            return;
        }
        for downstream_id in downstreams {
            tracing::info!(
                upstream_id = upstream.id,
                upstream_run_id,
                downstream_id,
                ?terminal,
                "dependency: firing downstream"
            );
            if let Err(error) = self.trigger_tx.send(LocalTrigger {
                job_id: downstream_id,
                trigger_by: Some(upstream_run_id),
                params_override: None,
                trigger_type: TriggerType::Workflow,
            }) {
                tracing::warn!(
                    ?error,
                    downstream_id,
                    "dependency local trigger dispatch failed"
                );
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
        // metrics: 触发记录 + 终态 discarded（in_flight 没 +1，所以也不用 -1）
        self.metrics.record_trigger(trigger_type);
        self.metrics.record_terminal(RunState::Discarded);
        let trace_id = Uuid::new_v4().to_string();
        let now = chrono::Local::now().naive_local();
        let active = sys_job_run::ActiveModel {
            id: NotSet,
            job_id: Set(job.id),
            trace_id: Set(trace_id),
            trigger_type: Set(trigger_type),
            trigger_by: Set(trigger_by),
            state: Set(RunState::Discarded),
            instance: Set(Some(self.instance.to_string())),
            scheduled_at: Set(scheduled_at),
            started_at: Set(None),
            finished_at: Set(Some(now)),
            retry_count: Set(retry_count as i32),
            result_json: Set(None),
            error_message: Set(Some("blocked by previous run (DISCARD policy)".into())),
            log_excerpt: Set(None),
            unique_key: Set(None),
            create_time: NotSet,
        };
        if let Err(error) = active.insert(&self.db).await {
            tracing::error!(?error, job_id = job.id, "record DISCARDED state failed");
        }
    }

    /// Unique 去重击中：写一条 DISCARDED 记录留痕，error_message 标注 unique conflict。
    #[allow(clippy::too_many_arguments)]
    async fn record_unique_skipped(
        &self,
        job: &sys_job::Model,
        trigger_type: TriggerType,
        trigger_by: Option<i64>,
        scheduled_at: NaiveDateTime,
        retry_count: u32,
        unique_key: String,
    ) {
        self.metrics.record_terminal(RunState::Discarded);
        let trace_id = Uuid::new_v4().to_string();
        let now = chrono::Local::now().naive_local();
        let active = sys_job_run::ActiveModel {
            id: NotSet,
            job_id: Set(job.id),
            trace_id: Set(trace_id),
            trigger_type: Set(trigger_type),
            trigger_by: Set(trigger_by),
            state: Set(RunState::Discarded),
            instance: Set(Some(self.instance.to_string())),
            scheduled_at: Set(scheduled_at),
            started_at: Set(None),
            finished_at: Set(Some(now)),
            retry_count: Set(retry_count as i32),
            result_json: Set(None),
            error_message: Set(Some(
                "unique conflict: 已有相同 unique_key 的 run 在执行".into(),
            )),
            log_excerpt: Set(None),
            unique_key: Set(Some(unique_key)),
            create_time: NotSet,
        };
        if let Err(error) = active.insert(&self.db).await {
            tracing::error!(?error, job_id = job.id, "record unique-skip state failed");
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
        unique_key: Option<String>,
    ) -> anyhow::Result<i64> {
        let active = sys_job_run::ActiveModel {
            id: NotSet,
            job_id: Set(job.id),
            trace_id: Set(trace_id.to_string()),
            trigger_type: Set(trigger_type),
            trigger_by: Set(trigger_by),
            state: Set(state),
            instance: Set(Some(self.instance.to_string())),
            scheduled_at: Set(scheduled_at),
            started_at: Set(Some(started_at)),
            finished_at: Set(None),
            retry_count: Set(retry_count as i32),
            result_json: Set(None),
            error_message: Set(None),
            log_excerpt: Set(None),
            unique_key: Set(unique_key),
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
        // metrics: 终态分桶 + in_flight -1
        self.metrics.record_terminal(state);
        self.metrics.dec_in_flight();
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

/// 单次执行的结果分类，决定是否触发重试。
#[derive(Debug, Clone, Copy)]
enum RunOutcome {
    Succeeded,
    Failed,
    Timeout,
    Canceled,
    /// 连 sys_job_run 都没创建成功（DB 故障）—— 不重试，避免风暴
    CreateRunFailed,
    /// Unique 去重击中：跳过本次，不重试
    UniqueSkipped,
}

impl RunOutcome {
    fn should_retry(&self) -> bool {
        matches!(self, Self::Failed | Self::Timeout)
    }
}

/// handler 调用结果的内部分类。
enum HandlerOutcome {
    Timeout(Duration),
    Canceled,
    Failed(String),
}

impl HandlerOutcome {
    fn from_handler_error(err: JobError) -> Self {
        match err {
            JobError::Timeout(d) => Self::Timeout(d),
            JobError::Canceled => Self::Canceled,
            other => Self::Failed(other.to_string()),
        }
    }
}

/// 把 `catch_unwind` 捕获的 panic payload 还原成可读消息。
fn format_panic(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        format!("handler panicked: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("handler panicked: {s}")
    } else {
        "handler panicked (unknown payload)".to_string()
    }
}
