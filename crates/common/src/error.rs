use spring_web::ProblemDetails;

pub type ApiResult<T, E = ApiErrors> = Result<T, E>;

#[derive(Debug, thiserror::Error, ProblemDetails)]
pub enum ApiErrors {
    #[status_code(400)]
    #[error("{0}")]
    BadRequest(String),

    #[status_code(401)]
    #[problem_type("about:blank")]
    #[error("{0}")]
    Unauthorized(String),

    #[status_code(403)]
    #[problem_type("about:blank")]
    #[error("{0}")]
    Forbidden(String),

    #[status_code(404)]
    #[error("{0}")]
    NotFound(String),

    #[status_code(409)]
    #[error("{0}")]
    Conflict(String),

    #[status_code(422)]
    #[error("{0}")]
    ValidationFailed(String),

    #[status_code(429)]
    #[error("{0}")]
    TooManyRequests(String),

    #[status_code(500)]
    #[problem_type("about:blank")]
    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[status_code(503)]
    #[problem_type("about:blank")]
    #[error("{0}")]
    ServiceUnavailable(String),
}
