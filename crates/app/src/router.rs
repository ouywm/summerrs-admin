use std::collections::HashMap;

use axum_client_ip::ClientIpSource;
use summer_ai::summer_ai_admin::admin_group;
use summer_ai::summer_ai_relay::{ApiKeyStrategy, relay_group};
use summer_auth::{GroupAuthLayer, JwtStrategy};
use summer_system::system_group;
use summer_web::Router;
use summer_web::axum::body;
use summer_web::axum::extract::Request;
use summer_web::axum::middleware::{self, Next};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::handler::auto_grouped_routers;
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

    let api_router = system_router
        .merge(admin_router)
        .layer(middleware::from_fn(problem_middleware));

    if !by_group.is_empty() {
        let mut unknown: Vec<String> = by_group.keys().cloned().collect();
        unknown.sort();
        panic!("unsupported auth groups: {}", unknown.join(", "));
    }

    Router::new()
        .nest("/api", api_router)
        .merge(relay_router)
        .merge(grouped.default)
        .layer(ClientIpSource::ConnectInfo.into_extension())
}

async fn problem_middleware(request: Request, next: Next) -> Response {
    let uri = request.uri().path().to_string();
    let response = next.run(request).await;
    let status = response.status();

    if status.is_client_error() || status.is_server_error() {
        let body = response.into_body();
        let body = body::to_bytes(body, usize::MAX)
            .await
            .expect("server body read failed");
        let detail = String::from_utf8(body.to_vec())
            .unwrap_or_else(|_| "read body to string failed".to_string());
        let title = status.canonical_reason().unwrap_or("error");

        summer_web::problem_details::ProblemDetails::new("http-error", title, status.as_u16())
            .with_instance(uri)
            .with_detail(detail)
            .into_response()
    } else {
        response
    }
}
