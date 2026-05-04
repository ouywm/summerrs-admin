use std::sync::Arc;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::app::{App, AppBuilder};
use summer::async_trait;
use summer::error::Result;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_job::JobScheduler;
use summer_sea_orm::DbConn;

use crate::engine::{
    DynamicScheduler, LocalTriggerReceiver, SchedulerHandle, SchedulerMetrics, Worker,
    evaluate_misfire, instance_id,
};
use crate::entity::sys_job;
use crate::registry::{BuiltinJob, HandlerRegistry};
use crate::service::{DependencyService, JobService};

pub struct SummerSchedulerPlugin;

#[async_trait]
impl Plugin for SummerSchedulerPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let registry = Arc::new(HandlerRegistry::collect());

        tracing::info!(
            count = registry.len(),
            handlers = ?registry.names(),
            "summer-job-dynamic: handler registry collected"
        );

        app.add_component(registry);
        app.add_component(SchedulerHandle::default());
        app.add_component(SchedulerMetrics::new());
        app.add_scheduler(|app: Arc<App>| Box::new(Self::start(app)));
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_job::JobPlugin", "summer_sea_orm::SeaOrmPlugin"]
    }
}

impl SummerSchedulerPlugin {
    async fn start(app: Arc<App>) -> Result<String> {
        let cron_sched = app.get_expect_component::<JobScheduler>();
        let registry = app.get_expect_component::<Arc<HandlerRegistry>>();
        let db = app.get_expect_component::<DbConn>();
        let handle = app.get_expect_component::<SchedulerHandle>();
        let job_svc = app.get_expect_component::<JobService>();
        let dep_svc = app.get_expect_component::<DependencyService>();

        let metrics = app.get_expect_component::<Arc<SchedulerMetrics>>();
        let (trigger_tx, trigger_rx) = crate::engine::local_trigger::channel();
        let worker = Worker::new(
            db.clone(),
            registry,
            app.clone(),
            metrics,
            trigger_tx,
            dep_svc,
        );
        let scheduler = DynamicScheduler::new(cron_sched, worker);

        handle.install(scheduler.clone()).await;
        spawn_local_trigger_loop(scheduler.clone(), db.clone(), trigger_rx);

        // 内置任务 import：已存在则保留 DB 配置，不覆盖运维改动。
        let mut imported = 0usize;
        let mut import_failed = 0usize;
        for builtin in inventory::iter::<BuiltinJob> {
            let dto = (builtin.dto_factory)();
            let name = dto.name.clone();
            match job_svc.import_builtin_if_absent(dto).await {
                Ok(_) => imported += 1,
                Err(error) => {
                    import_failed += 1;
                    tracing::error!(builtin = %name, ?error, "import builtin job failed");
                }
            }
        }
        tracing::info!(
            imported,
            import_failed,
            "summer-job-dynamic: builtin job import done"
        );

        match sys_job::Entity::find()
            .filter(sys_job::Column::Enabled.eq(true))
            .all(&db)
            .await
        {
            Ok(jobs) => {
                if let Err(error) = scheduler.load_and_register_all(jobs.clone()).await {
                    tracing::error!(?error, "load_and_register_all failed");
                }
                run_misfire_sweep(&scheduler, &db, &jobs).await;
            }
            Err(error) => {
                tracing::error!(?error, "load enabled jobs failed");
            }
        }

        Ok(format!(
            "summer-job-dynamic single-instance scheduler started (instance={})",
            instance_id()
        ))
    }
}

fn spawn_local_trigger_loop(scheduler: DynamicScheduler, db: DbConn, mut rx: LocalTriggerReceiver) {
    tokio::spawn(async move {
        while let Some(trigger) = rx.recv().await {
            match sys_job::Entity::find_by_id(trigger.job_id).one(&db).await {
                Ok(Some(model)) => {
                    scheduler
                        .trigger_now(
                            &model,
                            trigger.trigger_type,
                            trigger.trigger_by,
                            trigger.params_override,
                        )
                        .await;
                }
                Ok(None) => {
                    tracing::warn!(job_id = trigger.job_id, "local trigger job not found");
                }
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        job_id = trigger.job_id,
                        "local trigger job load failed"
                    );
                }
            }
        }
    });
}

/// 启动时对已加载的 enabled jobs 跑一遍 misfire 评估，逐个补跑（spawn 异步）。
///
/// - 仅 cron 任务参与
/// - `MisfireStrategy::FireNow` + 错过 ≥1 次 → 补跑一次（trigger_type=Misfire）
/// - 错过多次也只补一次（防风暴）
/// - 用 spawn 不阻塞启动钩子，并保证 fire_misfire 内部 acquire blocking 排队跟正常 cron tick 一致
async fn run_misfire_sweep(scheduler: &DynamicScheduler, db: &DbConn, jobs: &[sys_job::Model]) {
    let mut fired = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    for job in jobs {
        match evaluate_misfire(db, job).await {
            Ok(decision) if decision.should_fire => {
                tracing::info!(
                    job_id = job.id,
                    name = %job.name,
                    missed = decision.missed_count,
                    baseline = ?decision.baseline,
                    previous = ?decision.previous_scheduled,
                    "misfire FIRE_NOW: scheduling catch-up run"
                );
                let scheduler = scheduler.clone();
                let job_owned = job.clone();
                tokio::spawn(async move {
                    scheduler.fire_misfire(&job_owned).await;
                });
                fired += 1;
            }
            Ok(_) => {
                skipped += 1;
            }
            Err(error) => {
                tracing::warn!(
                    job_id = job.id,
                    name = %job.name,
                    ?error,
                    "misfire evaluate failed"
                );
                failed += 1;
            }
        }
    }
    tracing::info!(fired, skipped, failed, "misfire sweep done");
}
