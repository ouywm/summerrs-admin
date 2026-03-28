use std::ops::Deref;

use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::{IntoResponse, Response};

use crate::{ShardingConnection, tenant::TenantContext};

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

#[derive(Debug, Clone)]
pub struct TenantShardingConnection(pub ShardingConnection);

impl Deref for TenantShardingConnection {
    type Target = ShardingConnection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for TenantShardingConnection {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let sharding = parts
            .extensions
            .get::<ShardingConnection>()
            .cloned()
            .ok_or_else(missing_tenant_sharding_connection)?;
        Ok(Self(sharding))
    }
}

impl summer_web::aide::OperationInput for TenantShardingConnection {}

fn missing_tenant() -> Response {
    summer_web::problem_details::ProblemDetails::new(
        "tenant-context-missing",
        "Internal Server Error",
        500,
    )
    .with_detail("租户上下文未初始化")
    .into_response()
}

fn missing_tenant_sharding_connection() -> Response {
    summer_web::problem_details::ProblemDetails::new(
        "tenant-sharding-connection-missing",
        "Internal Server Error",
        500,
    )
    .with_detail("租户分片连接未初始化")
    .into_response()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use sea_orm::ConnectionTrait;
    use sea_orm::{DbBackend, MockDatabase};
    use summer_web::axum::{extract::FromRequestParts, http::StatusCode};

    use crate::{
        config::TenantIsolationLevel,
        datasource::DataSourcePool,
        web::{CurrentTenant, OptionalCurrentTenant, TenantShardingConnection},
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

    #[tokio::test]
    async fn tenant_sharding_connection_extractor_reads_inserted_connection() {
        let mut parts = build_parts();
        let config = Arc::new(
            crate::ShardingConfig::from_test_str(
                r#"
                [datasources.ds_test]
                uri = "postgres://localhost/mock"
                schema = "test"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"
                "#,
            )
            .expect("config"),
        );
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([(
                "ds_test".to_string(),
                MockDatabase::new(DbBackend::Postgres).into_connection(),
            )]),
        )
        .expect("pool");
        parts.extensions.insert(
            crate::ShardingConnection::with_pool(config, pool).expect("sharding connection"),
        );

        let sharding = TenantShardingConnection::from_request_parts(&mut parts, &())
            .await
            .expect("tenant sharding connection");

        assert_eq!(
            sharding.get_database_backend(),
            sea_orm::DbBackend::Postgres
        );
    }

    #[tokio::test]
    async fn tenant_sharding_connection_extractor_rejects_when_absent() {
        let mut parts = build_parts();

        let rejection = TenantShardingConnection::from_request_parts(&mut parts, &())
            .await
            .expect_err("missing sharding connection should reject");

        assert_eq!(rejection.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
