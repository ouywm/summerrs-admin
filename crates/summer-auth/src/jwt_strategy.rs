//! `JwtStrategy` —— admin 域的 JWT 鉴权策略。
//!
//! 对应原 [`crate::middleware::AuthLayer`] 的语义：
//!
//! - 有 token → 验证 token → 成功注入 `UserSession`；失败时若为强鉴权路径返 401/403，
//!   否则放行（保留"豁免路径上带 token 也解析出用户信息"的行为）
//! - 无 token + 强鉴权路径 → 401
//! - 无 token + 豁免路径 → 放行
//!
//! 失败响应格式采用 `ProblemDetails`（RFC 7807）——跟 admin 其他接口对齐。

use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::{Authorization, HeaderMapExt};
use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::http;
use summer_web::axum::response::IntoResponse;
use summer_web::axum::response::Response;
use summer_web::extractor::RequestPartsExt;
use summer_web::problem_details::ProblemDetails;

use crate::config::AuthConfig;
use crate::error::AuthError;
use crate::path_auth::PathAuthConfig;
use crate::session::SessionManager;
use crate::session::model::{UserProfile, UserSession, ValidatedAccess};
use crate::strategy::GroupAuthStrategy;

/// admin JWT 鉴权策略。挂到 `env!("CARGO_PKG_NAME")` 对应的 group 上。
#[derive(Clone)]
pub struct JwtStrategy {
    path_config: PathAuthConfig,
    group: &'static str,
}

impl JwtStrategy {
    pub fn new(path_config: PathAuthConfig, group: &'static str) -> Self {
        Self { path_config, group }
    }

    /// 默认配置：`include = "/**"`，`exclude` 取自该 group 下 `#[public]` / `#[no_auth]`
    /// 编译期注册的 [`crate::public_routes::PublicRoute`]。绝大多数业务域用这个就够。
    pub fn for_group(group: &'static str) -> Self {
        Self::for_group_with(group, PathAuthConfig::new().include("/**"))
    }

    /// 在调用方提供的 [`PathAuthConfig`] 之上，自动并入该 group 下 inventory 注册的
    /// public routes，再绑定到指定 group。适合 app 入口集中声明各域的 include/exclude。
    pub fn for_group_with(group: &'static str, cfg: PathAuthConfig) -> Self {
        Self::new(cfg.extend_excludes_from_public_routes(group), group)
    }
}

#[async_trait::async_trait]
impl GroupAuthStrategy for JwtStrategy {
    fn group(&self) -> &'static str {
        self.group
    }

    fn path_config(&self) -> &PathAuthConfig {
        &self.path_config
    }

    async fn authenticate(&self, req: &mut Request<Body>) -> Result<(), Response<Body>> {
        let method = req.method().clone();
        let path = req.uri().path().to_string();

        // 从 AppState 获取 SessionManager
        let (parts, body) = std::mem::take(req).into_parts();
        let manager = parts
            .get_component::<SessionManager>()
            .expect("SessionManager not found in AppState");
        *req = Request::from_parts(parts, body);

        let config = manager.config();

        let requires_auth = self.path_config.requires_auth(&method, &path);

        let token = extract_token(req, config);

        let Some(token) = token else {
            return if requires_auth {
                Err(unauthorized_response())
            } else {
                Ok(())
            };
        };

        match manager.validate_token(&token).await {
            Ok(validated) => {
                let session = UserSession {
                    login_id: validated.login_id,
                    device: validated.device.clone(),
                    profile: build_profile_from_validated(&validated),
                };
                req.extensions_mut().insert(session);
                Ok(())
            }
            Err(AuthError::AccountBanned) if requires_auth => Err(banned_response()),
            Err(AuthError::RefreshRequired) if requires_auth => Err(refresh_required_response()),
            Err(_) if requires_auth => Err(unauthorized_response()),
            Err(_) => Ok(()),
        }
    }
}

fn extract_token(req: &Request, config: &AuthConfig) -> Option<String> {
    if let Some(t) = extract_token_from_header(req, config) {
        return Some(t);
    }
    if config.is_read_cookie
        && let Some(t) = extract_token_from_cookie(req, config)
    {
        return Some(t);
    }
    None
}

fn extract_token_from_header(req: &Request, config: &AuthConfig) -> Option<String> {
    let header_name = &config.token_name;

    if header_name.eq_ignore_ascii_case(http::header::AUTHORIZATION.as_str())
        && let Some(Authorization(bearer)) = req.headers().typed_get::<Authorization<Bearer>>()
    {
        return Some(bearer.token().to_string());
    }

    let header = req.headers().get(header_name)?;
    let value = header.to_str().ok()?;

    if let Some(ref prefix) = config.token_prefix
        && !prefix.is_empty()
        && let Some(token) = value.strip_prefix(prefix.as_str())
    {
        return Some(token.to_string());
    }

    Some(value.to_string())
}

fn extract_token_from_cookie(req: &Request, config: &AuthConfig) -> Option<String> {
    let cookie_header = req.headers().get(http::header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;
    let cookie_name = config.cookie_name.as_deref().unwrap_or(&config.token_name);

    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some((name, value)) = pair.split_once('=')
            && name.trim() == cookie_name
        {
            return Some(value.trim().to_string());
        }
    }

    None
}

fn build_profile_from_validated(validated: &ValidatedAccess) -> UserProfile {
    UserProfile {
        user_name: validated.user_name.clone(),
        nick_name: validated.nick_name.clone(),
        roles: validated.roles.clone(),
        permissions: validated.permissions.clone(),
    }
}

fn unauthorized_response() -> Response<Body> {
    ProblemDetails::new("not-authenticated", "Unauthorized", 401)
        .with_detail("未登录或登录已过期")
        .into_response()
}

fn banned_response() -> Response<Body> {
    ProblemDetails::new("account-banned", "Forbidden", 403)
        .with_detail("账号已被封禁")
        .into_response()
}

fn refresh_required_response() -> Response<Body> {
    ProblemDetails::new("token-refresh-required", "Unauthorized", 401)
        .with_detail("Token 需要刷新")
        .into_response()
}
