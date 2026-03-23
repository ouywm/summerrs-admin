use sea_orm::DatabaseConnection;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, Plugin};

pub struct SystemSchemaSyncPlugin;

#[async_trait]
impl Plugin for SystemSchemaSyncPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let enabled = cfg!(debug_assertions) || schema_sync_env_enabled();
        if !enabled {
            tracing::info!(
                "System schema sync skipped; enable with debug build or SUMMER_SYSTEM_SCHEMA_SYNC=1"
            );
            return;
        }

        let db: DatabaseConnection = app.get_component::<DatabaseConnection>().expect(
            "DatabaseConnection 未找到，请确保 SeaOrmPlugin 在 SystemSchemaSyncPlugin 之前注册",
        );

        let prefix = summer_system_model::schema_registry_prefix();
        summer_system_model::sync_schema(&db)
            .await
            .unwrap_or_else(|error| panic!("system schema sync failed for {prefix}: {error}"));

        tracing::info!("System schema synced from entity definitions: {prefix}");
    }

    fn name(&self) -> &str {
        "system-schema-sync"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}

fn schema_sync_env_enabled() -> bool {
    match std::env::var("SUMMER_SYSTEM_SCHEMA_SYNC") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        ),
        Err(_) => false,
    }
}
