use std::ops::Deref;

use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::{IntoResponse, Response};

use crate::tenant::TenantContext;

#[derive(Debug, Clone)]
pub struct CurrentTenant(pub TenantContext);

impl Deref for CurrentTenant {
    type Target = TenantContext;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for CurrentTenant {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let tenant = parts
            .extensions
            .get::<TenantContext>()
            .cloned()
            .ok_or_else(missing_tenant)?;
        Ok(Self(tenant))
    }
}

impl summer_web::aide::OperationInput for CurrentTenant {}

pub struct OptionalCurrentTenant(pub Option<CurrentTenant>);

impl<S: Send + Sync> FromRequestParts<S> for OptionalCurrentTenant {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<TenantContext>()
                .cloned()
                .map(CurrentTenant),
        ))
    }
}

impl summer_web::aide::OperationInput for OptionalCurrentTenant {}

fn missing_tenant() -> Response {
    summer_web::problem_details::ProblemDetails::new(
        "tenant-context-missing",
        "Internal Server Error",
        500,
    )
    .with_detail("租户上下文未初始化")
    .into_response()
}

#[cfg(test)]
mod tests {
    use summer_web::axum::{extract::FromRequestParts, http::StatusCode};

    use crate::{
        config::TenantIsolationLevel,
        web::{CurrentTenant, OptionalCurrentTenant},
    };

    fn build_parts() -> summer_web::axum::http::request::Parts {
        summer_web::axum::http::Request::builder()
            .uri("/tenant")
            .body(())
            .expect("request")
            .into_parts()
            .0
    }

    #[tokio::test]
    async fn current_tenant_extractor_reads_inserted_context() {
        let mut parts = build_parts();
        parts.extensions.insert(crate::TenantContext::new(
            "T-REQ-EXTRACTOR",
            TenantIsolationLevel::SharedRow,
        ));

        let tenant = CurrentTenant::from_request_parts(&mut parts, &())
            .await
            .expect("current tenant");

        assert_eq!(tenant.tenant_id, "T-REQ-EXTRACTOR");
        assert_eq!(tenant.isolation_level, TenantIsolationLevel::SharedRow);
    }

    #[tokio::test]
    async fn optional_current_tenant_extractor_returns_some_when_present() {
        let mut parts = build_parts();
        parts.extensions.insert(crate::TenantContext::new(
            "T-REQ-EXTRACTOR",
            TenantIsolationLevel::SharedRow,
        ));

        let tenant = OptionalCurrentTenant::from_request_parts(&mut parts, &())
            .await
            .expect("optional tenant");

        assert_eq!(
            tenant.0.expect("tenant").tenant_id,
            "T-REQ-EXTRACTOR".to_string()
        );
    }

    #[tokio::test]
    async fn optional_current_tenant_extractor_returns_none_when_absent() {
        let mut parts = build_parts();

        let tenant = OptionalCurrentTenant::from_request_parts(&mut parts, &())
            .await
            .expect("optional tenant");

        assert!(tenant.0.is_none());
    }

    #[tokio::test]
    async fn current_tenant_extractor_rejects_when_absent() {
        let mut parts = build_parts();

        let rejection = CurrentTenant::from_request_parts(&mut parts, &())
            .await
            .expect_err("missing tenant should reject");

        assert_eq!(rejection.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
