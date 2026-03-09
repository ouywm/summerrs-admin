use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::http;
use summer_web::problem_details::ProblemDetails;
use summer_web::axum::response::Response;
use tower_layer::Layer;

use crate::config::AuthConfig;
use crate::path_auth::PathAuthConfig;
use crate::session::SessionManager;

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
                    Ok(login_id) => {
                        // 检查用户类型限制
                        if let Some(path_config) = &path_config {
                            if let Some(allowed_types) =
                                path_config.allowed_user_types(&path)
                            {
                                if !allowed_types.contains(&login_id.user_type) {
                                    return Ok(forbidden_response());
                                }
                            }
                        }

                        // auto_renew：验证成功后续期 access token 的 TTL
                        if config.auto_renew {
                            manager.renew_access_token(&token, &login_id).await;
                        }

                        // 加载完整 UserSession 并注入 extensions
                        match manager.get_session(&login_id).await {
                            Ok(Some(session)) => {
                                req.extensions_mut().insert(session);
                            }
                            Ok(None) => {
                                tracing::warn!("Session not found for authenticated user: {:?}", login_id);
                            }
                            Err(e) => {
                                tracing::error!("Failed to load session for {:?}: {}", login_id, e);
                            }
                        }

                        // 注入 LoginId 到 extensions
                        req.extensions_mut().insert(login_id);
                    }
                    Err(_) if requires_auth => {
                        // Token 无效 + 需要鉴权 → 401
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
    if config.is_read_cookie {
        if let Some(token) = extract_token_from_cookie(req, config) {
            return Some(token);
        }
    }

    None
}

/// 从 Header 提取 token
fn extract_token_from_header(req: &Request, config: &AuthConfig) -> Option<String> {
    let header_name = &config.token_name;
    let header = req.headers().get(header_name)?;
    let value = header.to_str().ok()?;

    // 如果配置了 token_prefix，先尝试去除前缀
    if let Some(ref prefix) = config.token_prefix {
        if !prefix.is_empty() {
            if let Some(token) = value.strip_prefix(prefix.as_str()) {
                return Some(token.to_string());
            }
        }
    }

    // 没有前缀或前缀不匹配时，直接使用整个值
    Some(value.to_string())
}

/// 从 Cookie 提取 token
fn extract_token_from_cookie(req: &Request, config: &AuthConfig) -> Option<String> {
    let cookie_header = req.headers().get(http::header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;

    let cookie_name = config
        .cookie_name
        .as_deref()
        .unwrap_or(&config.token_name);

    // 解析 Cookie: name1=value1; name2=value2
    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some((name, value)) = pair.split_once('=') {
            if name.trim() == cookie_name {
                return Some(value.trim().to_string());
            }
        }
    }

    None
}

/// 构建 401 未授权响应（RFC 7807 ProblemDetails 格式）
fn unauthorized_response() -> Response<Body> {
    use summer_web::axum::response::IntoResponse;
    ProblemDetails::new("not-authenticated", "Unauthorized", 401)
        .with_detail("未登录或登录已过期")
        .into_response()
}

/// 构建 403 禁止访问响应（RFC 7807 ProblemDetails 格式）
fn forbidden_response() -> Response<Body> {
    use summer_web::axum::response::IntoResponse;
    ProblemDetails::new("forbidden", "Forbidden", 403)
        .with_detail("无权访问该资源")
        .into_response()
}
