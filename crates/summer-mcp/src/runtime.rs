use std::{future::Future, sync::Arc, time::Duration};

use rmcp::ServiceExt;
use sea_orm::{DatabaseConnection, DbBackend};

use crate::{
    config::{McpConfig, McpHttpMode, McpTransport},
    server::AdminMcpServer,
};

pub type McpRuntimeError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub async fn run_server(config: McpConfig, db: DatabaseConnection) -> Result<(), McpRuntimeError> {
    run_server_with_shutdown(config, db, std::future::pending()).await
}

pub async fn run_server_with_shutdown<F>(
    config: McpConfig,
    db: DatabaseConnection,
    shutdown: F,
) -> Result<(), McpRuntimeError>
where
    F: Future<Output = ()> + Send + 'static,
{
    validate_database_backend(&db)?;
    match (config.transport.clone(), config.http_mode.clone()) {
        (McpTransport::Stdio, _) => run_stdio(config, db, shutdown).await,
        (McpTransport::Http, McpHttpMode::Embedded) => Err(std::io::Error::other(
            "embedded MCP HTTP mode must be mounted into an existing app router",
        )
        .into()),
        (McpTransport::Http, McpHttpMode::Standalone) => run_http(config, db, shutdown).await,
    }
}

pub(crate) fn validate_database_backend(db: &DatabaseConnection) -> Result<(), McpRuntimeError> {
    if db.get_database_backend() != DbBackend::Postgres {
        return Err(std::io::Error::other("summer-mcp currently supports PostgreSQL only").into());
    }
    Ok(())
}

struct HttpServiceComponents {
    session_manager:
        Arc<rmcp::transport::streamable_http_server::session::local::LocalSessionManager>,
    server_config: rmcp::transport::streamable_http_server::StreamableHttpServerConfig,
}

fn build_http_service_components(
    config: &McpConfig,
    cancellation_token: tokio_util::sync::CancellationToken,
) -> HttpServiceComponents {
    let session_manager = Arc::new(
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager {
            session_config:
                rmcp::transport::streamable_http_server::session::local::SessionConfig {
                    channel_capacity: config.session_channel_capacity,
                    keep_alive: config.session_keep_alive.map(Duration::from_secs),
                },
            ..Default::default()
        },
    );

    let server_config = rmcp::transport::streamable_http_server::StreamableHttpServerConfig {
        sse_keep_alive: Some(Duration::from_secs(config.sse_keep_alive)),
        sse_retry: Some(Duration::from_secs(config.sse_retry)),
        stateful_mode: config.stateful_mode,
        json_response: config.json_response,
        cancellation_token,
    };

    HttpServiceComponents {
        session_manager,
        server_config,
    }
}

pub fn mount_http_service_on_router(
    router: summer_web::Router,
    config: McpConfig,
    db: DatabaseConnection,
) -> summer_web::Router {
    let path = config.path.clone();
    let components =
        build_http_service_components(&config, tokio_util::sync::CancellationToken::new());

    let service_config = config.clone();
    let service = rmcp::transport::streamable_http_server::StreamableHttpService::new(
        move || Ok(AdminMcpServer::new(&service_config, db.clone())),
        components.session_manager,
        components.server_config,
    );

    tracing::info!(
        "MCP HTTP service mounted on existing app router at {}",
        path
    );

    router.nest_service(&path, service)
}

async fn run_stdio<F>(
    config: McpConfig,
    db: DatabaseConnection,
    shutdown: F,
) -> Result<(), McpRuntimeError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let server = AdminMcpServer::new(&config, db);
    tracing::info!("MCP server starting in stdio mode");

    let running = server.serve(rmcp::transport::stdio()).await?;
    tokio::select! {
        result = running.waiting() => {
            let _ = result;
        }
        _ = shutdown => {
            tracing::info!("received shutdown signal for MCP stdio server");
        }
    }

    tracing::info!("MCP stdio server stopped");
    Ok(())
}

async fn run_http<F>(
    config: McpConfig,
    db: DatabaseConnection,
    shutdown: F,
) -> Result<(), McpRuntimeError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let bind_addr = std::net::SocketAddr::new(config.binding, config.port);
    let path = config.path.clone();
    let ct = tokio_util::sync::CancellationToken::new();

    let components = build_http_service_components(&config, ct.child_token());
    let service_config = config.clone();
    let service = rmcp::transport::streamable_http_server::StreamableHttpService::new(
        move || Ok(AdminMcpServer::new(&service_config, db.clone())),
        components.session_manager,
        components.server_config,
    );

    let router = summer_web::axum::Router::new().nest_service(&path, service);

    tracing::info!("MCP server listening on http://{bind_addr}{path}");

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    summer_web::axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown.await;
            tracing::info!("shutting down MCP HTTP server...");
            ct.cancel();
        })
        .await?;

    tracing::info!("MCP HTTP server stopped");
    Ok(())
}
