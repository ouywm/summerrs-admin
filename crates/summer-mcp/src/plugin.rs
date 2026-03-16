use sea_orm::DatabaseConnection;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, Plugin};
use summer_sea_orm::config::SeaOrmConfig;
use summer_web::LayerConfigurator;

use crate::{
    config::{McpConfig, McpHttpMode, McpTransport},
    run_server,
    runtime::{mount_http_service_on_router, validate_database_backend},
};

pub struct McpPlugin;

#[async_trait]
impl Plugin for McpPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let mut config = app
            .get_config::<McpConfig>()
            .expect("mcp plugin config load failed");

        if !config.enabled {
            tracing::info!("MCP plugin is disabled, skipping");
            return;
        }

        let db: DatabaseConnection = app
            .get_component::<DatabaseConnection>()
            .expect("DatabaseConnection 未找到，请确保 SeaOrmPlugin 在 McpPlugin 之前注册");

        validate_database_backend(&db).expect("summer-mcp currently supports PostgreSQL only");

        if config.default_database_url.is_none() {
            if let Ok(sea_orm_config) = app.get_config::<SeaOrmConfig>() {
                config.default_database_url = Some(sea_orm_config.uri);
            }
        }

        match (config.transport.clone(), config.http_mode.clone()) {
            (McpTransport::Http, McpHttpMode::Embedded) => {
                let route_config = config.clone();
                let route_db = db.clone();
                app.add_router_layer(move |router| {
                    mount_http_service_on_router(router, route_config.clone(), route_db.clone())
                });
                tracing::info!(
                    "MCP plugin initialized (transport: http, mode: embedded, path: {})",
                    config.path
                );
            }
            (transport, mode) => {
                let transport_for_task = transport.clone();
                let server_handle = tokio::spawn(async move {
                    match run_server(config, db).await {
                        Ok(()) => {
                            tracing::info!("MCP {transport_for_task} server stopped normally")
                        }
                        Err(error) => {
                            tracing::error!(
                                "MCP {transport_for_task} server exited with error: {error}; \
                                 the admin MCP service is no longer available"
                            );
                        }
                    }
                });

                // Monitor the server task so panics are surfaced instead of silently dropped.
                tokio::spawn(async move {
                    if let Err(join_error) = server_handle.await {
                        tracing::error!(
                            "MCP server task terminated unexpectedly: {join_error}; \
                             the admin MCP service is no longer available"
                        );
                    }
                });

                tracing::info!("MCP plugin initialized (transport: {transport}, mode: {mode})");
            }
        }
    }

    fn name(&self) -> &str {
        "mcp"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}
