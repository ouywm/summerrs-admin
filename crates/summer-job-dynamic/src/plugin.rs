use std::sync::Arc;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::app::{App, AppBuilder};
use summer::async_trait;
use summer::error::Result;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_job::JobScheduler;
use summer_sea_orm::DbConn;

use crate::entity::sys_job;
use crate::registry::{BuiltinJob, HandlerRegistry};
use crate::scheduler::{DynamicScheduler, SchedulerHandle, Worker, instance_id};
use crate::service::JobService;

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

        let worker = Worker::new(db.clone(), registry, app.clone());
        let scheduler = DynamicScheduler::new(cron_sched, worker);

        handle.install(scheduler.clone()).await;

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
                if let Err(error) = scheduler.load_and_register_all(jobs).await {
                    tracing::error!(?error, "load_and_register_all failed");
                }
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
