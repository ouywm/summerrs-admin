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

        // sea-schema 的 PostgreSQL discovery future 不是 Send，无法跨越本
        // async fn(#[async_trait] 要求 Send)的 await 点。把它整体隔离到
        // block_in_place 内执行，!Send future 在闭包里创建并消费完毕，
        // 不逃逸到外层 Send future，也避免跨 runtime 使用 SQLx pool。
        let sync_db = db.clone();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(summer_system_model::sync_schema(&sync_db))
        })
        .unwrap_or_else(|error| panic!("entity schema sync failed: {error}"));

        tracing::info!("Entity schema synced from entity definitions");
    }

    fn dependencies(&self) -> Vec<&str> {
        vec![std::any::type_name::<summer_sea_orm::SeaOrmPlugin>()]
    }
}
