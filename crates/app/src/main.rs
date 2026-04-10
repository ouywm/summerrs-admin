use axum_client_ip::ClientIpSource;
use summer::App;
use summer::auto_config;
use summer_auth::{PathAuthBuilder, SummerAuthConfigurator, SummerAuthPlugin};
use summer_job::JobConfigurator;
use summer_job::JobPlugin;
use summer_mail::MailPlugin;
use summer_mcp::McpPlugin;
use summer_redis::RedisPlugin;
use summer_sea_orm::SeaOrmPlugin;
use summer_sharding::SummerShardingPlugin;
use summer_web::LayerConfigurator;
use summer_web::WebConfigurator;
use summer_web::WebPlugin;
use summer_web::axum::body::Body;
use summer_web::axum::http;
use tower_http::catch_panic::CatchPanicLayer;

use summer_plugins::{BackgroundTaskPlugin, Ip2RegionPlugin, LogBatchCollectorPlugin, S3Plugin};
use summer_sql_rewrite::SummerSqlRewritePlugin;
use summer_system::plugins::{PermBitmapPlugin, RateLimitPlugin, SocketGatewayPlugin};

fn app_path_auth_builder() -> PathAuthBuilder {
    PathAuthBuilder::new()
        .include("/**")
        .exclude("/auth/login")
        .exclude("/auth/refresh")
        .exclude("/api/v1/**")
        .exclude("/v1/**")
}

#[auto_config(WebConfigurator, JobConfigurator)]
#[tokio::main]
async fn main() {
    App::new()
        .add_plugin(WebPlugin)
        .add_plugin(SeaOrmPlugin)
        .add_plugin(RedisPlugin)
        .add_plugin(SummerAuthPlugin)
        .add_plugin(SummerShardingPlugin)
        .add_plugin(SummerSqlRewritePlugin)
        // .add_plugin(EntitySchemaSyncPlugin)
        .add_plugin(JobPlugin)
        .add_plugin(MailPlugin)
        .add_plugin(RateLimitPlugin)
        .add_plugin(PermBitmapPlugin)
        .add_plugin(SocketGatewayPlugin)
        .add_plugin(Ip2RegionPlugin)
        .add_plugin(S3Plugin)
        .add_plugin(BackgroundTaskPlugin)
        .add_plugin(LogBatchCollectorPlugin)
        .add_plugin(McpPlugin)
        .auth_configure(app_path_auth_builder())
        .add_router_layer(|router| {
            router
                .layer(ClientIpSource::ConnectInfo.into_extension())
                .layer(CatchPanicLayer::custom(handle_panic))
        })
        .run()
        .await;
}

/// 全局 panic 处理：将 panic 转为 ProblemDetails (RFC 7807) 格式响应
fn handle_panic(err: Box<dyn std::any::Any + Send + 'static>) -> http::Response<Body> {
    use summer_web::axum::response::IntoResponse;

    let detail = if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        s.to_string()
    } else {
        "Unknown internal error".to_string()
    };

    tracing::error!("Service panicked: {detail}");

    summer_web::problem_details::ProblemDetails::new("internal-error", "Internal Server Error", 500)
        .with_detail(detail)
        .into_response()
}
