use std::any::Any;
use std::collections::HashMap;

use axum_client_ip::ClientIpSource;
use summer_ai::summer_ai_admin::admin_group;
use summer_ai::summer_ai_relay::{ApiKeyStrategy, relay_group};
use summer_auth::{GroupAuthLayer, JwtStrategy};
use summer_system::system_group;
use summer_web::Router;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::handler::auto_grouped_routers;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};

pub fn router() -> Router {
    let grouped = auto_grouped_routers();
    let mut by_group = grouped.by_group;

    // 取出 group 名下注册的路由；缺失则返回空 Router（使该 group 完全无 handler 时不致崩）。
    let take = |g: &'static str, m: &mut HashMap<String, Router>| -> Router {
        m.remove(g).unwrap_or_default()
    };

    // 各域当前都用默认路径策略（`/**` + inventory public exclude）。
    // 未来若要为某个 group 自定义白名单/黑名单，把这里换成
    // `JwtStrategy::for_group_with(group, PathAuthConfig::new().include(..).exclude(..))`。
    let relay_router = take(relay_group(), &mut by_group)
        .layer(GroupAuthLayer::new(
            ApiKeyStrategy::for_group(relay_group()),
        ))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid));

    let system_router = take(system_group(), &mut by_group)
        .layer(GroupAuthLayer::new(JwtStrategy::for_group(system_group())));

    let admin_router = take(admin_group(), &mut by_group)
        .layer(GroupAuthLayer::new(JwtStrategy::for_group(admin_group())));

    let api_router = system_router.merge(admin_router);

    if !by_group.is_empty() {
        let mut unknown: Vec<String> = by_group.keys().cloned().collect();
        unknown.sort();
        panic!("unsupported auth groups: {}", unknown.join(", "));
    }

    // CatchPanicLayer 仅覆盖 admin / system / default —— relay 域的 5xx 必须保持上游协议风格
    // （OpenAI / Claude / Gemini 各自的 error JSON 由 RelayError 自己构造），不能被这里转成
    // RFC 7807。relay 真发生 panic 时让 hyper 自然返空 500，连接行为与上游 500 一致。
    Router::new()
        .nest("/api", api_router)
        .merge(grouped.default)
        .layer(CatchPanicLayer::custom(handle_panic))
        .merge(relay_router)
        .layer(ClientIpSource::ConnectInfo.into_extension())
}

/// 全局 panic 兜底（仅 admin / system / default 域）：把 panic 转成 RFC 7807 ProblemDetails 500 响应，
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
