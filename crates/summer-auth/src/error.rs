use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("未登录或登录已过期")]
    NotLogin,

    #[error("Token 无效")]
    InvalidToken,

    #[error("Token 已过期")]
    TokenExpired,

    #[error("Refresh Token 无效或已过期")]
    InvalidRefreshToken,

    #[error("无此角色: {0}")]
    NoRole(String),

    #[error("无此权限: {0}")]
    NoPermission(String),

    #[error("会话不存在")]
    SessionNotFound,

    #[error("超过最大设备数限制: {0}")]
    MaxDevicesExceeded(usize),

    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("内部错误: {0}")]
    Internal(String),

    #[error("QR 码不存在或已过期")]
    QrCodeNotFound,

    #[error("QR 码状态不正确")]
    QrCodeInvalidState,
}

pub type AuthResult<T> = Result<T, AuthError>;

// ── IntoResponse 实现（web feature） ──

#[cfg(feature = "web")]
impl summer_web::axum::response::IntoResponse for AuthError {
    fn into_response(self) -> summer_web::axum::response::Response {
        let (problem_type, title, status) = match &self {
            AuthError::NotLogin | AuthError::InvalidToken | AuthError::TokenExpired
            | AuthError::InvalidRefreshToken | AuthError::SessionNotFound => {
                ("not-authenticated", "Unauthorized", 401u16)
            }
            AuthError::NoRole(_) | AuthError::NoPermission(_) => {
                ("forbidden", "Forbidden", 403)
            }
            AuthError::QrCodeNotFound | AuthError::QrCodeInvalidState => {
                ("bad-request", "Bad Request", 400)
            }
            AuthError::MaxDevicesExceeded(_) => {
                ("max-devices-exceeded", "Conflict", 409)
            }
            AuthError::StorageError(_) | AuthError::Internal(_) => {
                ("internal-error", "Internal Server Error", 500)
            }
        };

        summer_web::problem_details::ProblemDetails::new(problem_type, title, status)
            .with_detail(self.to_string())
            .into_response()
    }
}
