use summer_web::ProblemDetails;

pub type ApiResult<T, E = ApiErrors> = Result<T, E>;

#[derive(Debug, thiserror::Error, ProblemDetails)]
pub enum ApiErrors {
    #[status_code(400)]
    #[problem_type("bad-request")]
    #[error("{0}")]
    BadRequest(String),

    #[status_code(401)]
    #[problem_type("not-authenticated")]
    #[error("{0}")]
    Unauthorized(String),

    #[status_code(403)]
    #[problem_type("forbidden")]
    #[error("{0}")]
    Forbidden(String),

    #[status_code(404)]
    #[problem_type("not-found")]
    #[error("{0}")]
    NotFound(String),

    #[status_code(409)]
    #[problem_type("conflict")]
    #[error("{0}")]
    Conflict(String),

    /// 分片上传不完整（前端应调用 list_parts 获取缺失分片并续传）
    #[status_code(409)]
    #[problem_type("multipart-incomplete")]
    #[error("{0}")]
    IncompleteUpload(String),

    #[status_code(422)]
    #[problem_type("validation-failed")]
    #[error("{0}")]
    ValidationFailed(String),

    #[status_code(429)]
    #[problem_type("too-many-requests")]
    #[error("{0}")]
    TooManyRequests(String),

    #[status_code(500)]
    #[problem_type("internal-error")]
    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[status_code(503)]
    #[problem_type("service-unavailable")]
    #[error("{0}")]
    ServiceUnavailable(String),
}
