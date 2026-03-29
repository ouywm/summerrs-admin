use std::time::Duration;

use sea_orm::EntityTrait;
use summer_ai_model::entity::log;
use summer_plugins::background_task::BackgroundTaskConfig;
use summer_plugins::background_task::typed_batch::{TypedBatchQueue, TypedBatchQueueBuilder};
use summer_plugins::log_batch_collector::LogBatchConfig;
use summer_sea_orm::DbConn;

#[derive(Clone)]
pub struct AiLogBatchQueue(TypedBatchQueue);

impl AiLogBatchQueue {
    pub fn build(
        db: DbConn,
        task_config: &BackgroundTaskConfig,
        batch_config: &LogBatchConfig,
    ) -> Self {
        let queue = TypedBatchQueueBuilder::new()
            .task_capacity(task_config.capacity)
            .task_workers(task_config.workers)
            .register_batch::<log::ActiveModel, _, _>(
                batch_config.batch_size,
                Duration::from_millis(batch_config.flush_interval_ms),
                batch_config.capacity,
                {
                    let db = db.clone();
                    move |batch| {
                        let db = db.clone();
                        async move {
                            if let Err(error) = log::Entity::insert_many(batch).exec(&db).await {
                                tracing::error!("failed to batch persist AI usage logs: {error}");
                            }
                        }
                    }
                },
            )
            .build();

        Self(queue)
    }

    #[cfg(test)]
    pub(crate) fn immediate(db: DbConn) -> Self {
        Self(
            TypedBatchQueueBuilder::new()
                .register_batch::<log::ActiveModel, _, _>(
                    1,
                    Duration::from_millis(1),
                    16,
                    move |batch| {
                        let db = db.clone();
                        async move {
                            if let Err(error) = log::Entity::insert_many(batch).exec(&db).await {
                                tracing::error!(
                                    "failed to persist AI usage logs immediately in tests: {error}"
                                );
                            }
                        }
                    },
                )
                .build(),
        )
    }

    pub fn push(&self, model: log::ActiveModel) {
        self.0.push(model);
    }
}
