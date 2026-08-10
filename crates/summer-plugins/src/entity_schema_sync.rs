use sea_orm::{ConnectionTrait, DatabaseConnection};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, Plugin};

pub struct EntitySchemaSyncPlugin;

#[async_trait]
impl Plugin for EntitySchemaSyncPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let db: DatabaseConnection = app
            .get_component::<DatabaseConnection>()
            .expect(
                "DatabaseConnection not found; ensure SeaOrmPlugin is registered before EntitySchemaSyncPlugin",
            );

        db.execute_unprepared("CREATE SCHEMA IF NOT EXISTS sys")
            .await
            .expect("failed to create sys schema before entity schema sync");

        summer_system_model::sync_schema(&db)
            .await
            .unwrap_or_else(|error| panic!("entity schema sync failed: {error}"));

        tracing::info!("Entity schema synced from entity definitions");
    }

    fn dependencies(&self) -> Vec<&str> {
        vec![std::any::type_name::<summer_sea_orm::SeaOrmPlugin>()]
    }
}
