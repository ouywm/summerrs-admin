use std::any::Any;

use axum_client_ip::ClientIpSource;
use summer_web::Router;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::handler::auto_grouped_routers;
use tower_http::catch_panic::CatchPanicLayer;

/// 拼装最终 axum [`Router`]。
///
/// app crate 只负责收集 inventory 路由并按域分发:
///
/// - `summer-system` group —— 交给 `summer-system::router_with_layers` 挂 JWT 和资源权限
/// - default group —— 没显式 group 的 handler,直接合并到根 router
pub fn router() -> Router {
    let mut grouped = auto_grouped_routers();

    let api_router =
        summer_system::router_with_layers(grouped.take_group(summer_system::system_group()));

    let default_router = grouped.take_default();

    Router::new()
        .nest("/api", api_router)
        .merge(default_router)
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(ClientIpSource::ConnectInfo.into_extension())
}

/// 全局 panic 兜底(仅 system / default 域):把 panic 转成 RFC 7807 ProblemDetails 500 响应,
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
