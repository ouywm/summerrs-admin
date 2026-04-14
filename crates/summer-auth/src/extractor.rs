use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::IntoResponse;

use crate::session::model::{UserProfile, UserSession};
use crate::user_type::LoginId;

/// 用户提取器 — 从 `request.extensions` 提取 `UserSession`
///
/// 未登录时返回 401。
/// 用于 logout/refresh 以及其他只要求已登录、不区分用户类型的路由。
pub struct LoginUser {
    pub login_id: LoginId,
    pub profile: UserProfile,
    pub session: UserSession,
}

impl<S: Send + Sync> FromRequestParts<S> for LoginUser {
    type Rejection = summer_web::axum::response::Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let session = parts
            .extensions
            .get::<UserSession>()
            .cloned()
            .ok_or_else(unauthorized)?;

        Ok(Self {
            login_id: session.login_id,
            profile: session.profile.clone(),
            session,
        })
    }
}

impl LoginUser {
    /// 返回当前登录用户的角色列表
    pub fn roles(&self) -> &[String] {
        self.profile.roles()
    }

    /// 返回当前登录用户的权限列表
    pub fn permissions(&self) -> &[String] {
        self.profile.permissions()
    }
}

/// `LoginUser` 对 OpenAPI 文档透明
impl summer_web::aide::OperationInput for LoginUser {}

/// 可选用户提取器
pub struct OptionalLoginUser(pub Option<LoginUser>);

impl<S: Send + Sync> FromRequestParts<S> for OptionalLoginUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match LoginUser::from_request_parts(parts, state).await {
            Ok(user) => Ok(Self(Some(user))),
            Err(_) => Ok(Self(None)),
        }
    }
}

/// `OptionalLoginUser` 对 OpenAPI 文档透明
impl summer_web::aide::OperationInput for OptionalLoginUser {}

/// 401 未登录响应
fn unauthorized() -> summer_web::axum::response::Response {
    summer_web::problem_details::ProblemDetails::new("not-authenticated", "Unauthorized", 401)
        .with_detail("未登录或登录已过期")
        .into_response()
}
