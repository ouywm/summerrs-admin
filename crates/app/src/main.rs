mod plugin;
mod router;
mod service;

use crate::plugin::background_task::BackgroundTaskPlugin;
use crate::plugin::ip2region_plugin::Ip2RegionPlugin;
use crate::plugin::log_batch_collector::LogBatchCollectorPlugin;
use crate::plugin::sea_orm_plugin::SeaOrmPlugin;
use axum_client_ip::ClientIpSource;
use spring::auto_config;
use spring::App;
use spring_job::JobConfigurator;
use spring_job::JobPlugin;
use spring_redis::RedisPlugin;
use spring_sa_token::{PathAuthBuilder, SaTokenAuthConfigurator, SaTokenPlugin};
use spring_web::axum::body::Body;
use spring_web::axum::http;
use spring_web::LayerConfigurator;
use spring_web::WebConfigurator;
use spring_web::WebPlugin;
use tower_http::catch_panic::CatchPanicLayer;

#[auto_config(WebConfigurator, JobConfigurator)]
#[tokio::main]
async fn main() {
    App::new()
        .add_plugin(WebPlugin)
        .add_plugin(SeaOrmPlugin)
        .add_plugin(RedisPlugin)
        .add_plugin(JobPlugin)
        .add_plugin(SaTokenPlugin)
        .add_plugin(Ip2RegionPlugin)
        .add_plugin(BackgroundTaskPlugin)
        .add_plugin(LogBatchCollectorPlugin)
        .sa_token_auth(PathAuthBuilder::new().include("/**").exclude("/auth/login"))
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
    let detail = if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        s.to_string()
    } else {
        "Unknown internal error".to_string()
    };

    tracing::error!("Service panicked: {detail}");

    let body = serde_json::json!({
        "type": "about:blank",
        "title": "Internal Server Error",
        "status": 500,
        "detail": detail
    });

    http::Response::builder()
        .status(http::StatusCode::INTERNAL_SERVER_ERROR)
        .header("content-type", "application/problem+json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
