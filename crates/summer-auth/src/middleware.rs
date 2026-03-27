use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::http;
use summer_web::axum::response::IntoResponse;
use summer_web::axum::response::Response;
use summer_web::problem_details::ProblemDetails;
use tower_layer::Layer;

use crate::config::AuthConfig;
use crate::error::AuthError;
use crate::path_auth::PathAuthConfig;
use crate::session::SessionManager;
use crate::session::model::{
    AdminProfile, BusinessProfile, CustomerProfile, UserProfile, UserSession,
};
use crate::user_type::UserType;

/// AuthLayer — Axum Layer
#[derive(Clone)]
pub struct AuthLayer {
    manager: SessionManager,
    path_config: Option<PathAuthConfig>,
}

impl AuthLayer {
    pub fn new(manager: SessionManager, path_config: Option<PathAuthConfig>) -> Self {
        Self {
            manager,
            path_config,
        }
    }
}

impl<S: Clone> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            manager: self.manager.clone(),
            path_config: self.path_config.clone(),
        }
    }
}

/// AuthMiddleware — 实际的中间件服务
#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    manager: SessionManager,
    path_config: Option<PathAuthConfig>,
}

impl<S> tower_service::Service<Request> for AuthMiddleware<S>
where
    S: tower_service::Service<Request, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let manager = self.manager.clone();
        let path_config = self.path_config.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let path = req.uri().path().to_string();
            let config = manager.config();

            // 检查路径是否需要鉴权
            let requires_auth = match &path_config {
                Some(config) => config.requires_auth(&path),
                None => true, // 无配置时默认需要鉴权
            };

            // 提取 token：优先 Header，其次 Cookie
            let token = extract_token(&req, config);

            if let Some(token) = token {
                // 有 token，尝试验证
                match manager.validate_token(&token).await {
                    Ok(validated) => {
                        // 检查用户类型限制
                        if let Some(path_config) = &path_config
                            && let Some(allowed_types) = path_config.allowed_user_types(&path)
                            && !allowed_types.contains(&validated.login_id.user_type)
                        {
                            return Ok(forbidden_response());
                        }

                        // 从 ValidatedAccess 构造 UserSession 并注入 extensions
                        let login_id = validated.login_id.clone();
                        let device = validated.device.clone();
                        let tenant_id = validated.tenant_id.clone();
                        let profile = build_profile_from_validated(&validated);
                        let session = UserSession {
                            login_id: validated.login_id.clone(),
                            device,
                            tenant_id,
                            profile,
                        };

                        req.extensions_mut().insert(session);
                        req.extensions_mut().insert(login_id);
                    }
                    Err(AuthError::AccountBanned) if requires_auth => {
                        return Ok(banned_response());
                    }
                    Err(AuthError::AccountBanned) => {
                        // 不需要鉴权的路径，封禁用户也继续
                    }
                    Err(AuthError::RefreshRequired) if requires_auth => {
                        return Ok(refresh_required_response());
                    }
                    Err(AuthError::RefreshRequired) => {
                        // 不需要鉴权的路径，继续
                    }
                    Err(_) if requires_auth => {
                        return Ok(unauthorized_response());
                    }
                    Err(_) => {
                        // Token 无效 + 不需要鉴权 → 继续
                    }
                }
            } else if requires_auth {
                // 无 token + 需要鉴权 → 401
                return Ok(unauthorized_response());
            }

            inner.call(req).await
        })
    }
}

/// 从请求中提取 token（Header 优先，Cookie 备选）
fn extract_token(req: &Request, config: &AuthConfig) -> Option<String> {
    // 1. 优先从 Header 提取
    if let Some(token) = extract_token_from_header(req, config) {
        return Some(token);
    }

    // 2. 从 Cookie 提取（如果启用）
    // TODO: 启用 Cookie 模式时需要实现 CSRF 防护
    // 方案：登录时签发 CSRF token，前端在 Header 中携带，中间件双重校验
    if config.is_read_cookie
        && let Some(token) = extract_token_from_cookie(req, config)
    {
        return Some(token);
    }

    None
}

/// 从 Header 提取 token
fn extract_token_from_header(req: &Request, config: &AuthConfig) -> Option<String> {
    let header_name = &config.token_name;
    let header = req.headers().get(header_name)?;
    let value = header.to_str().ok()?;

    // 如果配置了 token_prefix，先尝试去除前缀
    if let Some(ref prefix) = config.token_prefix
        && !prefix.is_empty()
        && let Some(token) = value.strip_prefix(prefix.as_str())
    {
        return Some(token.to_string());
    }

    // 没有前缀或前缀不匹配时，直接使用整个值
    Some(value.to_string())
}

/// 从 Cookie 提取 token
fn extract_token_from_cookie(req: &Request, config: &AuthConfig) -> Option<String> {
    let cookie_header = req.headers().get(http::header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;

    let cookie_name = config.cookie_name.as_deref().unwrap_or(&config.token_name);

    // 解析 Cookie: name1=value1; name2=value2
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

/// 根据 ValidatedAccess 中的 login_id 用户类型构建 UserProfile
fn build_profile_from_validated(validated: &crate::session::model::ValidatedAccess) -> UserProfile {
    match validated.login_id.user_type {
        UserType::Admin => UserProfile::Admin(AdminProfile {
            user_name: validated.user_name.clone(),
            nick_name: validated.nick_name.clone(),
            roles: validated.roles.clone(),
            permissions: validated.permissions.clone(),
        }),
        UserType::Business => UserProfile::Business(BusinessProfile {
            user_name: validated.user_name.clone(),
            nick_name: validated.nick_name.clone(),
            roles: validated.roles.clone(),
            permissions: validated.permissions.clone(),
        }),
        UserType::Customer => UserProfile::Customer(CustomerProfile {
            nick_name: validated.nick_name.clone(),
        }),
    }
}

/// 构建 401 未授权响应
fn unauthorized_response() -> Response<Body> {
    ProblemDetails::new("not-authenticated", "Unauthorized", 401)
        .with_detail("未登录或登录已过期")
        .into_response()
}

/// 构建 403 禁止访问响应
fn forbidden_response() -> Response<Body> {
    ProblemDetails::new("forbidden", "Forbidden", 403)
        .with_detail("无权访问该资源")
        .into_response()
}

/// 构建 403 封禁响应
fn banned_response() -> Response<Body> {
    ProblemDetails::new("account-banned", "Forbidden", 403)
        .with_detail("账号已被封禁")
        .into_response()
}

/// 构建 401 需要刷新响应
fn refresh_required_response() -> Response<Body> {
    ProblemDetails::new("token-refresh-required", "Unauthorized", 401)
        .with_detail("Token 需要刷新")
        .into_response()
}
