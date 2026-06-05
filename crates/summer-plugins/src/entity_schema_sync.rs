use sea_orm::DatabaseConnection;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, Plugin};

pub struct EntitySchemaSyncPlugin;

#[async_trait]
impl Plugin for EntitySchemaSyncPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let enabled = cfg!(debug_assertions) || schema_sync_env_enabled();
        if !enabled {
            tracing::info!(
                "Entity schema sync skipped; enable with debug build or SUMMER_ENTITY_SCHEMA_SYNC=1"
            );
            return;
        }

        let db: DatabaseConnection = app
            .get_component::<DatabaseConnection>()
            .expect(
                "DatabaseConnection not found; ensure SeaOrmPlugin is registered before EntitySchemaSyncPlugin",
            );

        let system_prefix = summer_system_model::schema_registry_prefix();
        // sea-schema 的 PostgreSQL discovery future 不是 Send，无法跨越本
        // async fn(#[async_trait] 要求 Send)的 await 点。把它整体隔离到
        // spawn_blocking 的独立线程 + 当前线程 runtime 内执行，!Send future
        // 在闭包里创建并消费完毕，永不逃逸到外层 Send future。
        let sync_db = db.clone();
        let result = tokio::task::spawn_blocking(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build schema-sync runtime")
                .block_on(summer_system_model::sync_schema(&sync_db))
        })
        .await
        .expect("schema-sync task panicked");
        finish_sync(system_prefix, result);
    }

    fn name(&self) -> &str {
        "entity-schema-sync"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}

fn finish_sync(prefix: String, result: Result<(), sea_orm::DbErr>) {
    result.unwrap_or_else(|error| panic!("entity schema sync failed for {prefix}: {error}"));

    tracing::info!("Entity schema synced from entity definitions: {prefix}");
}

fn schema_sync_env_enabled() -> bool {
    ["SUMMER_SYSTEM_SCHEMA_SYNC"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
}
