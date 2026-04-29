use axum_client_ip::ClientIpSource;
use summer_ai::summer_ai_relay::ApiKeyStrategy;
use summer_auth::path_auth::{PathAuthConfig, PathAuthConfigs, RouteRule};
use summer_auth::public_routes::public_routes_in_group;
use summer_auth::{GroupAuthLayer, JwtStrategy, PathAuthBuilder};
use summer_web::Router;
use summer_web::axum::body;
use summer_web::axum::extract::Request;
use summer_web::axum::middleware::{self, Next};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::handler::auto_grouped_routers;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};

fn summer_ai_relay_group() -> &'static str {
    summer_ai::summer_ai_relay::relay_group()
}

fn summer_ai_admin_group() -> &'static str {
    summer_ai::summer_ai_admin::admin_group()
}

fn summer_system_group() -> &'static str {
    summer_system::system_group()
}

/// 构建三类鉴权域（system / ai-admin / ai-relay）的基础 include 规则。
fn auth_path_config() -> PathAuthBuilder {
    PathAuthBuilder::new()
        .add_group(PathAuthBuilder::group(summer_system_group()).include("/**"))
        .add_group(PathAuthBuilder::group(summer_ai_admin_group()).include("/**"))
        .add_group(PathAuthBuilder::group(summer_ai_relay_group()).include("/**"))
}

pub fn router() -> Router {
    // 按 group 生成鉴权配置，再将自动收集的分组路由分发到统一入口。
    let mut path_auth_configs = auth_path_config().build();
    let grouped = auto_grouped_routers();
    let default_router = grouped.default;
    let mut grouped_routers = grouped.by_group;

    let relay_router = grouped_routers
        .remove(summer_ai_relay_group())
        .map(|router| build_relay_router(&mut path_auth_configs, router))
        .unwrap_or_default();
    let ai_admin_router = grouped_routers
        .remove(summer_ai_admin_group())
        .map(|router| build_jwt_router(&mut path_auth_configs, summer_ai_admin_group(), router))
        .unwrap_or_default();
    let system_router = grouped_routers
        .remove(summer_system_group())
        .map(|router| build_jwt_router(&mut path_auth_configs, summer_system_group(), router))
        .unwrap_or_default();

    if !grouped_routers.is_empty() {
        let mut unknown_groups: Vec<String> = grouped_routers.keys().cloned().collect();
        unknown_groups.sort();
        panic!("unsupported auth groups: {}", unknown_groups.join(", "));
    }
    let api_router = system_router.merge(ai_admin_router);

    Router::new()
        .nest(
            "/api",
            api_router.layer(middleware::from_fn(problem_middleware)),
        )
        .merge(relay_router)
        .merge(default_router)
        .layer(ClientIpSource::ConnectInfo.into_extension())
}

fn build_relay_router(path_auth_configs: &mut PathAuthConfigs, group_router: Router) -> Router {
    let relay_cfg = path_auth_configs
        .get_mut(summer_ai_relay_group())
        .expect("path auth config for summer-ai-relay not found");
    merge_public_router(summer_ai_relay_group(), relay_cfg);

    let api_key_strategy = ApiKeyStrategy::new(relay_cfg.clone(), summer_ai_relay_group());
    group_router
        .layer(GroupAuthLayer::new(api_key_strategy))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

fn build_jwt_router(
    path_auth_configs: &mut PathAuthConfigs,
    group: &'static str,
    group_router: Router,
) -> Router {
    let cfg = path_auth_configs
        .get_mut(group)
        .unwrap_or_else(|| panic!("path auth config for {group} not found"));
    merge_public_router(group, cfg);

    let strategy = JwtStrategy::new(cfg.clone(), group);
    group_router.layer(GroupAuthLayer::new(strategy))
}

/// 根据 group 合并公开路由到 path_config
pub fn merge_public_router(group: &str, path_auth_config: &mut PathAuthConfig) {
    for r in public_routes_in_group(group) {
        let rule = RouteRule::new(r.method, r.pattern.to_string());
        if !path_auth_config.exclude.contains(&rule) {
            path_auth_config.exclude.push(rule);
        }
    }
    path_auth_config.rebuild_param_route_cache()
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
