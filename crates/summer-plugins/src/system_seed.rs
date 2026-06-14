use sea_orm::DatabaseConnection;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, Plugin};
use summer_migration::{Migrator, MigratorTrait};

pub struct SystemSeedPlugin;

#[async_trait]
impl Plugin for SystemSeedPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        if !cfg!(debug_assertions) {
            tracing::info!("System seed skipped outside debug build");
            return;
        }

        let db: DatabaseConnection = app
            .get_component::<DatabaseConnection>()
            .expect("DatabaseConnection not found; ensure SeaOrmPlugin is registered before SystemSeedPlugin");

        Migrator::up(&db, None)
            .await
            .unwrap_or_else(|error| panic!("system seed failed: {error}"));
    }

    fn name(&self) -> &str {
        "system-seed"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec![
            std::any::type_name::<summer_sea_orm::SeaOrmPlugin>(),
            std::any::type_name::<crate::entity_schema_sync::EntitySchemaSyncPlugin>(),
        ]
    }
}
