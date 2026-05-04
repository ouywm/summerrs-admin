use std::any::Any;

use axum_client_ip::ClientIpSource;
use summer_ai::summer_ai_admin;
use summer_ai::summer_ai_relay;
use summer_web::Router;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::handler::auto_grouped_routers;
use tower_http::catch_panic::CatchPanicLayer;

/// 拼装最终 axum [`Router`]。
///
/// 每个 crate 自己负责"路由 + 自家中间件"打包(`router_with_layers`),app crate
/// 只做拼装:
///
/// - `summer-ai-relay::router_with_layers()` —— 内部按入口协议分子路由,各自挂
///   `ApiKeyStrategy`(flavor 硬绑) + `panic_guard`(flavor 硬绑) + 共享 `RequestId`
/// - `summer-ai-admin::router_with_layers()` —— 挂 JWT
/// - `summer-system::router_with_layers()` —— 挂 JWT
/// - `summer-job-dynamic::router_with_layers()` —— 动态调度器 admin API,挂 JWT
/// - `auto_grouped_routers().default` —— 没显式 group 的 handler
///
/// 全局 [`CatchPanicLayer`] 仅覆盖 admin / system / scheduler / default 域,
/// 它们的 panic 转 RFC 7807。relay 域的 panic 由各家自己的 `*_panic_guard` 在
/// `CatchPanicLayer` 之前抓走,输出 OpenAI / Claude / Gemini 风格的错误 JSON。
pub fn router() -> Router {
    let api_router = summer_system::router_with_layers()
        .merge(summer_ai_admin::router_with_layers())
        .merge(summer_job_dynamic::router_with_layers());

    let default_router = auto_grouped_routers().default;

    Router::new()
        .nest("/api", api_router)
        .merge(default_router)
        .layer(CatchPanicLayer::custom(handle_panic))
        .merge(summer_ai_relay::router_with_layers())
        .layer(ClientIpSource::ConnectInfo.into_extension())
}

/// 全局 panic 兜底(仅 admin / system / default 域):把 panic 转成 RFC 7807 ProblemDetails 500 响应,
/// 避免连接被直接中断或返 axum 默认的 plain text。
fn handle_panic(err: Box<dyn Any + Send + 'static>) -> Response {
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
