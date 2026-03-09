use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::IntoResponse;

use crate::session::model::{
    AdminProfile, BusinessProfile, CustomerProfile, UserProfile, UserSession,
};
use crate::user_type::LoginId;

/// 通用登录用户提取器 — 从 request.extensions 提取 UserSession
///
/// 未登录时返回 401。
/// 用于 logout/refresh 等与用户类型无关的路由。
pub struct LoginUser {
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

        Ok(LoginUser { session })
    }
}

impl LoginUser {
    pub fn login_id(&self) -> &LoginId {
        &self.session.login_id
    }

    pub fn profile(&self) -> &UserProfile {
        &self.session.profile
    }

    pub fn roles(&self) -> &[String] {
        self.session.profile.roles()
    }

    pub fn permissions(&self) -> &[String] {
        self.session.profile.permissions()
    }
}

/// LoginUser 对 OpenAPI 文档透明
#[cfg(feature = "openapi")]
impl summer_web::aide::OperationInput for LoginUser {}

/// 可选登录用户提取器
pub struct OptionalLoginUser(pub Option<LoginUser>);

impl<S: Send + Sync> FromRequestParts<S> for OptionalLoginUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let session = parts.extensions.get::<UserSession>().cloned();

        match session {
            Some(session) => Ok(OptionalLoginUser(Some(LoginUser { session }))),
            None => Ok(OptionalLoginUser(None)),
        }
    }
}

/// OptionalLoginUser 对 OpenAPI 文档透明
#[cfg(feature = "openapi")]
impl summer_web::aide::OperationInput for OptionalLoginUser {}

/// 为指定用户类型生成类型安全的 Axum 提取器
///
/// 用法：`define_user_extractor!(AdminUser, Admin, AdminProfile);`
/// 生成的提取器在 profile 类型不匹配时返回 403 Forbidden
macro_rules! define_user_extractor {
    ($name:ident, $variant:ident, $profile_ty:ty) => {
        pub struct $name {
            pub login_id: LoginId,
            pub profile: $profile_ty,
        }

        impl<S: Send + Sync> FromRequestParts<S> for $name {
            type Rejection = summer_web::axum::response::Response;

            async fn from_request_parts(
                parts: &mut Parts,
                _state: &S,
            ) -> Result<Self, Self::Rejection> {
                let session = parts
                    .extensions
                    .get::<UserSession>()
                    .cloned()
                    .ok_or_else(unauthorized)?;

                match session.profile {
                    UserProfile::$variant(profile) => Ok($name {
                        login_id: session.login_id,
                        profile,
                    }),
                    _ => Err(forbidden()),
                }
            }
        }

        #[cfg(feature = "openapi")]
        impl summer_web::aide::OperationInput for $name {}
    };
}

// 每种用户类型一行——新增类型时加一行即可
define_user_extractor!(AdminUser, Admin, AdminProfile);
define_user_extractor!(BusinessUser, Business, BusinessProfile);
define_user_extractor!(CustomerUser, Customer, CustomerProfile);

// ── 错误响应 ──

fn unauthorized() -> summer_web::axum::response::Response {
    summer_web::problem_details::ProblemDetails::new("not-authenticated", "Unauthorized", 401)
        .with_detail("未登录或登录已过期")
        .into_response()
}

fn forbidden() -> summer_web::axum::response::Response {
    summer_web::problem_details::ProblemDetails::new("forbidden", "Forbidden", 403)
        .with_detail("无权访问该资源")
        .into_response()
}
