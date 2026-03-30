use std::{sync::Arc, time::Duration};

use sea_orm::{DatabaseConnection, DbBackend};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
#[cfg(feature = "web")]
use summer_web::LayerConfigurator;

#[cfg(feature = "web")]
use crate::TenantContextLayer;
use crate::{
    ShardingConnection,
    config::SummerShardingConfig,
    rewrite_plugin::PluginRegistry,
    tenant::{
        PgTenantMetadataListener, TENANT_METADATA_CHANNEL, TenantMetadataListener,
        TenantMetadataNotificationHandler,
    },
};

const TENANT_METADATA_POLL_INTERVAL: Duration = Duration::from_secs(10);
const DISCOVERY_POLL_INTERVAL: Duration = Duration::from_secs(10);

pub struct SummerShardingPlugin;

#[async_trait]
impl Plugin for SummerShardingPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<SummerShardingConfig>()
            .expect("summer-sharding plugin config load failed");
        #[cfg(feature = "web")]
        let tenant_id_source = config.tenant.tenant_id_source;
        #[cfg(feature = "web")]
        let tenant_id_field = config.tenant.tenant_id_field.clone();
        #[cfg(feature = "web")]
        let default_isolation = config.tenant.default_isolation;

        if !config.enabled {
            tracing::info!("summer-sharding plugin is disabled, skipping");
            return;
        }

        let mut connection = ShardingConnection::build(
            config
                .into_runtime_config()
                .expect("summer-sharding runtime config build failed"),
        )
        .await
        .expect("summer-sharding connection build failed");

        // 注入 SQL 改写插件注册表（由 sharding_rewrite_configure 注册）
        if let Some(registry) = app.get_component::<PluginRegistry>() {
            tracing::info!(
                plugins = %registry.summary(),
                "injecting SQL rewrite plugin registry"
            );
            connection.set_plugin_registry(registry);
        }

        let metadata_connection: DatabaseConnection = app
            .get_component::<DatabaseConnection>()
            .expect("DatabaseConnection not found; ensure SeaOrmPlugin is registered before SummerShardingPlugin");

        connection
            .reload_tenant_metadata(&metadata_connection)
            .await
            .expect("tenant metadata reload failed");

        if metadata_connection.get_database_backend() == DbBackend::Postgres {
            let listener = Arc::new(PgTenantMetadataListener::new(TENANT_METADATA_CHANNEL));
            let listener_connection = connection.clone();
            let listener_metadata = metadata_connection.clone();
            let handler: TenantMetadataNotificationHandler = Arc::new(move |payload| {
                let listener_connection = listener_connection.clone();
                let listener_metadata = listener_metadata.clone();
                Box::pin(async move {
                    if let Err(error) = listener_connection
                        .apply_tenant_metadata_notification(&listener_metadata, payload.as_str())
                        .await
                    {
                        tracing::warn!(
                            error = ?error,
                            "tenant metadata notification handler failed"
                        );
                    }
                })
            });
            let _listener_handle = listener.spawn(metadata_connection.clone(), handler);
            tracing::info!("tenant metadata listener started");
        } else {
            tracing::info!(
                "tenant metadata listener skipped (backend {:?})",
                metadata_connection.get_database_backend()
            );
        }

        let _poll_handle = connection.spawn_tenant_metadata_polling(
            metadata_connection.clone(),
            TENANT_METADATA_POLL_INTERVAL,
        );
        let _ = connection
            .inner
            .pool
            .refresh_read_write_route_states()
            .await;
        let discovery_pool = connection.inner.pool.clone();
        let _discovery_handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(DISCOVERY_POLL_INTERVAL);
            loop {
                ticker.tick().await;
                let _ = discovery_pool.refresh_read_write_route_states().await;
            }
        });

        #[cfg(feature = "web")]
        let tenant_layer =
            TenantContextLayer::from_source_and_field(tenant_id_source, tenant_id_field)
                .with_default_isolation(default_isolation)
                .with_sharding(connection.clone());

        #[cfg(feature = "web")]
        app.add_router_layer(move |router| router.layer(tenant_layer.clone()));

        app.add_component(connection);
        tracing::info!("summer-sharding plugin initialized");
    }

    fn name(&self) -> &str {
        "summer_sharding::SummerShardingPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}
