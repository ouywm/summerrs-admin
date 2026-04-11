use thiserror::Error;

#[derive(Debug, Error, summer_web::ProblemDetails)]
pub enum AuthError {
    #[status_code(401)]
    #[problem_type("not-authenticated")]
    #[title("Unauthorized")]
    #[detail("未登录或登录已过期")]
    #[error("未登录或登录已过期")]
    NotLogin,

    #[status_code(401)]
    #[problem_type("not-authenticated")]
    #[title("Unauthorized")]
    #[detail("Token 无效")]
    #[error("Token 无效")]
    InvalidToken,

    #[status_code(401)]
    #[problem_type("not-authenticated")]
    #[title("Unauthorized")]
    #[detail("Token 已过期")]
    #[error("Token 已过期")]
    TokenExpired,

    #[status_code(401)]
    #[problem_type("not-authenticated")]
    #[title("Unauthorized")]
    #[detail("Refresh Token 无效或已过期")]
    #[error("Refresh Token 无效或已过期")]
    InvalidRefreshToken,

    #[status_code(401)]
    #[problem_type("not-authenticated")]
    #[title("Unauthorized")]
    #[detail("Refresh Token 已过期")]
    #[error("Refresh Token 已过期")]
    RefreshTokenExpired,

    #[status_code(403)]
    #[problem_type("account-banned")]
    #[title("Forbidden")]
    #[detail("账号已被封禁")]
    #[error("账号已被封禁")]
    AccountBanned,

    #[status_code(401)]
    #[problem_type("token-refresh-required")]
    #[title("Unauthorized")]
    #[detail("Token 需要刷新")]
    #[error("Token 需要刷新")]
    RefreshRequired,

    #[status_code(403)]
    #[problem_type("forbidden")]
    #[title("Forbidden")]
    #[error("无此角色: {0}")]
    NoRole(String),

    #[status_code(403)]
    #[problem_type("forbidden")]
    #[title("Forbidden")]
    #[error("无此权限: {0}")]
    NoPermission(String),

    #[status_code(401)]
    #[problem_type("not-authenticated")]
    #[title("Unauthorized")]
    #[detail("会话不存在")]
    #[error("会话不存在")]
    SessionNotFound,

    #[status_code(500)]
    #[problem_type("internal-error")]
    #[title("Internal Server Error")]
    #[error("存储错误: {0}")]
    StorageError(String),

    #[status_code(500)]
    #[problem_type("internal-error")]
    #[title("Internal Server Error")]
    #[error("内部错误: {0}")]
    Internal(String),

    #[status_code(400)]
    #[problem_type("bad-request")]
    #[title("Bad Request")]
    #[detail("QR 码不存在或已过期")]
    #[error("QR 码不存在或已过期")]
    QrCodeNotFound,

    #[status_code(400)]
    #[problem_type("bad-request")]
    #[title("Bad Request")]
    #[detail("QR 码状态不正确")]
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
