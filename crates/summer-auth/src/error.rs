use thiserror::Error;

#[derive(Debug, Error)]
#[cfg_attr(feature = "web", derive(summer_web::ProblemDetails))]
pub enum AuthError {
    #[cfg_attr(feature = "web", status_code(401))]
    #[cfg_attr(feature = "web", problem_type("not-authenticated"))]
    #[cfg_attr(feature = "web", title("Unauthorized"))]
    #[cfg_attr(feature = "web", detail("未登录或登录已过期"))]
    #[error("未登录或登录已过期")]
    NotLogin,

    #[cfg_attr(feature = "web", status_code(401))]
    #[cfg_attr(feature = "web", problem_type("not-authenticated"))]
    #[cfg_attr(feature = "web", title("Unauthorized"))]
    #[cfg_attr(feature = "web", detail("Token 无效"))]
    #[error("Token 无效")]
    InvalidToken,

    #[cfg_attr(feature = "web", status_code(401))]
    #[cfg_attr(feature = "web", problem_type("not-authenticated"))]
    #[cfg_attr(feature = "web", title("Unauthorized"))]
    #[cfg_attr(feature = "web", detail("Token 已过期"))]
    #[error("Token 已过期")]
    TokenExpired,

    #[cfg_attr(feature = "web", status_code(401))]
    #[cfg_attr(feature = "web", problem_type("not-authenticated"))]
    #[cfg_attr(feature = "web", title("Unauthorized"))]
    #[cfg_attr(feature = "web", detail("Refresh Token 无效或已过期"))]
    #[error("Refresh Token 无效或已过期")]
    InvalidRefreshToken,

    #[cfg_attr(feature = "web", status_code(401))]
    #[cfg_attr(feature = "web", problem_type("not-authenticated"))]
    #[cfg_attr(feature = "web", title("Unauthorized"))]
    #[cfg_attr(feature = "web", detail("Refresh Token 已过期"))]
    #[error("Refresh Token 已过期")]
    RefreshTokenExpired,

    #[cfg_attr(feature = "web", status_code(403))]
    #[cfg_attr(feature = "web", problem_type("account-banned"))]
    #[cfg_attr(feature = "web", title("Forbidden"))]
    #[cfg_attr(feature = "web", detail("账号已被封禁"))]
    #[error("账号已被封禁")]
    AccountBanned,

    #[cfg_attr(feature = "web", status_code(401))]
    #[cfg_attr(feature = "web", problem_type("token-refresh-required"))]
    #[cfg_attr(feature = "web", title("Unauthorized"))]
    #[cfg_attr(feature = "web", detail("Token 需要刷新"))]
    #[error("Token 需要刷新")]
    RefreshRequired,

    #[cfg_attr(feature = "web", status_code(403))]
    #[cfg_attr(feature = "web", problem_type("forbidden"))]
    #[cfg_attr(feature = "web", title("Forbidden"))]
    #[error("无此角色: {0}")]
    NoRole(String),

    #[cfg_attr(feature = "web", status_code(403))]
    #[cfg_attr(feature = "web", problem_type("forbidden"))]
    #[cfg_attr(feature = "web", title("Forbidden"))]
    #[error("无此权限: {0}")]
    NoPermission(String),

    #[cfg_attr(feature = "web", status_code(401))]
    #[cfg_attr(feature = "web", problem_type("not-authenticated"))]
    #[cfg_attr(feature = "web", title("Unauthorized"))]
    #[cfg_attr(feature = "web", detail("会话不存在"))]
    #[error("会话不存在")]
    SessionNotFound,

    #[cfg_attr(feature = "web", status_code(409))]
    #[cfg_attr(feature = "web", problem_type("max-devices-exceeded"))]
    #[cfg_attr(feature = "web", title("Conflict"))]
    #[error("超过最大设备数限制: {0}")]
    MaxDevicesExceeded(usize),

    #[cfg_attr(feature = "web", status_code(500))]
    #[cfg_attr(feature = "web", problem_type("internal-error"))]
    #[cfg_attr(feature = "web", title("Internal Server Error"))]
    #[error("存储错误: {0}")]
    StorageError(String),

    #[cfg_attr(feature = "web", status_code(500))]
    #[cfg_attr(feature = "web", problem_type("internal-error"))]
    #[cfg_attr(feature = "web", title("Internal Server Error"))]
    #[error("内部错误: {0}")]
    Internal(String),

    #[cfg_attr(feature = "web", status_code(400))]
    #[cfg_attr(feature = "web", problem_type("bad-request"))]
    #[cfg_attr(feature = "web", title("Bad Request"))]
    #[cfg_attr(feature = "web", detail("QR 码不存在或已过期"))]
    #[error("QR 码不存在或已过期")]
    QrCodeNotFound,

    #[cfg_attr(feature = "web", status_code(400))]
    #[cfg_attr(feature = "web", problem_type("bad-request"))]
    #[cfg_attr(feature = "web", title("Bad Request"))]
    #[cfg_attr(feature = "web", detail("QR 码状态不正确"))]
    #[error("QR 码状态不正确")]
    QrCodeInvalidState,
}

pub type AuthResult<T> = Result<T, AuthError>;

/// anyhow::Error → AuthError::StorageError 自动转换
/// 使得存储层调用可以直接 `storage.get_session(...).await?`
impl From<anyhow::Error> for AuthError {
    fn from(e: anyhow::Error) -> Self {
        AuthError::StorageError(e.to_string())
    }
}
