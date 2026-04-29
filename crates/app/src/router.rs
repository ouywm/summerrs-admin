use axum_client_ip::ClientIpSource;
use summer_ai::summer_ai_relay::ApiKeyStrategy;
use summer_auth::path_auth::{PathAuthConfig, RouteRule};
use summer_auth::public_routes::public_routes_in_group;
use summer_auth::{GroupAuthLayer, JwtStrategy, PathAuthBuilder};
use summer_web::Router;
use summer_web::axum::body;
use summer_web::axum::extract::Request;
use summer_web::axum::middleware::{self, Next};
use summer_web::axum::response::{IntoResponse, Response};

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
    // 先按 group 生成各自的路径鉴权配置，再分别挂载到对应路由树。
    let mut path_auth_configs = auth_path_config().build();

    let relay_router = {
        let relay_cfg = path_auth_configs
            .get_mut(summer_ai_relay_group())
            .expect("path auth config for summer-ai-relay not found");
        ai_relay_router(relay_cfg)
    };
    let ai_admin_group_router = {
        let ai_admin_cfg = path_auth_configs
            .get_mut(summer_ai_admin_group())
            .expect("path auth config for summer-ai-admin not found");
        ai_admin_router(ai_admin_cfg).layer(middleware::from_fn(problem_middleware))
    };
    let system_group_router = {
        let system_cfg = path_auth_configs
            .get_mut(summer_system_group())
            .expect("path auth config for summer-system not found");
        system_router(system_cfg).layer(middleware::from_fn(problem_middleware))
    };

    Router::new()
        .merge(relay_router)
        .merge(ai_admin_group_router)
        .merge(system_group_router)
        .layer(ClientIpSource::ConnectInfo.into_extension())
}

pub fn ai_relay_router(path_auth_config: &mut PathAuthConfig) -> Router {
    // relay 组使用 API Key 鉴权；公开路由通过 #[public]/#[no_auth] 合并到 exclude。
    merge_public_router(summer_ai_relay_group(), path_auth_config);

    let api_key_strategy = ApiKeyStrategy::new(path_auth_config.clone(), summer_ai_relay_group());
    summer_ai::summer_ai_relay::router::router().layer(GroupAuthLayer::new(api_key_strategy))
}

pub fn ai_admin_router(path_auth_config: &mut PathAuthConfig) -> Router {
    // ai-admin 组使用 JWT 鉴权。
    merge_public_router(summer_ai_admin_group(), path_auth_config);

    let strategy = JwtStrategy::new(path_auth_config.clone(), summer_ai_admin_group());
    summer_ai::summer_ai_admin::router::router().layer(GroupAuthLayer::new(strategy))
}

pub fn system_router(path_auth_config: &mut PathAuthConfig) -> Router {
    // system 组使用 JWT 鉴权。
    merge_public_router(summer_system_group(), path_auth_config);

    let strategy = JwtStrategy::new(path_auth_config.clone(), summer_system_group());
    summer_system::router::admin_router().layer(GroupAuthLayer::new(strategy))
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
