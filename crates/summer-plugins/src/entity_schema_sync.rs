use std::future::Future;
use std::pin::Pin;

use sea_orm::DatabaseConnection;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, Plugin};

pub struct EntitySchemaSyncPlugin;

type SchemaSyncFuture<'a> = Pin<Box<dyn Future<Output = Result<(), sea_orm::DbErr>> + Send + 'a>>;

struct RegisteredSchemaSync {
    name: &'static str,
    prefix: fn() -> String,
    sync: for<'a> fn(&'a DatabaseConnection) -> SchemaSyncFuture<'a>,
}

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

        for schema in registered_schema_syncs() {
            sync_registered_schema(&db, schema).await;
        }
    }

    fn name(&self) -> &str {
        "entity-schema-sync"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}

fn registered_schema_syncs() -> [RegisteredSchemaSync; 2] {
    [
        RegisteredSchemaSync {
            name: "system",
            prefix: summer_system_model::schema_registry_prefix,
            sync: system_schema_sync,
        },
        RegisteredSchemaSync {
            name: "ai",
            prefix: summer_ai_model::schema_registry_prefix,
            sync: ai_schema_sync,
        },
    ]
}

async fn sync_registered_schema(db: &DatabaseConnection, schema: RegisteredSchemaSync) {
    let prefix = (schema.prefix)();
    let result = (schema.sync)(db).await;
    result.unwrap_or_else(|error| panic!("entity schema sync failed for {prefix}: {error}"));

    tracing::info!(
        schema = schema.name,
        prefix,
        "Entity schema synced from entity definitions"
    );
}

fn system_schema_sync(db: &DatabaseConnection) -> SchemaSyncFuture<'_> {
    Box::pin(summer_system_model::sync_schema(db))
}

fn ai_schema_sync(db: &DatabaseConnection) -> SchemaSyncFuture<'_> {
    Box::pin(summer_ai_model::sync_schema(db))
}

fn schema_sync_env_enabled() -> bool {
    ["SUMMER_ENTITY_SCHEMA_SYNC", "SUMMER_SYSTEM_SCHEMA_SYNC"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{registered_schema_syncs, schema_sync_env_enabled};

    #[test]
    fn registered_schema_syncs_contains_system_and_ai() {
        let syncs = registered_schema_syncs();
        assert_eq!(syncs.len(), 2);
        assert_eq!(syncs[0].name, "system");
        assert_eq!(syncs[1].name, "ai");
    }

    #[test]
    fn schema_sync_env_enabled_accepts_truthy_values() {
        unsafe {
            std::env::set_var("SUMMER_ENTITY_SCHEMA_SYNC", "true");
        }
        assert!(schema_sync_env_enabled());
        unsafe {
            std::env::remove_var("SUMMER_ENTITY_SCHEMA_SYNC");
        }
    }
}
