#[cfg(any(test, feature = "web-probes"))]
use schemars::JsonSchema;
#[cfg(any(test, feature = "web-probes"))]
use sea_orm::{ConnectionTrait, DbBackend, Statement};
#[cfg(any(test, feature = "web-probes"))]
use sea_orm::QueryResult;
#[cfg(any(test, feature = "web-probes"))]
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "web-probes"))]
use summer_web::axum::Json;
#[cfg(any(test, feature = "web-probes"))]
use summer_web::error::{KnownWebError, WebError};
#[cfg(any(test, feature = "web-probes"))]
use summer_web::extractor::Component;
#[cfg(any(test, feature = "web-probes"))]
use summer_web::get_api;

#[cfg(any(test, feature = "web-probes"))]
use crate::{ShardingConnection, web::CurrentTenant};

#[cfg(any(test, feature = "web-probes"))]
const SHARED_TENANT_PROBE_SQL: &str =
    "SELECT id, tenant_id, payload FROM test.tenant_probe ORDER BY id";
#[cfg(any(test, feature = "web-probes"))]
const ISOLATED_TENANT_PROBE_SQL: &str =
    "SELECT id, payload FROM test.tenant_probe_isolated ORDER BY id";

#[cfg(any(test, feature = "web-probes"))]
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TenantContextProbeVo {
    pub tenant_id: String,
    pub isolation_level: String,
    pub datasource_override: Option<String>,
    pub schema_override: Option<String>,
}

#[cfg(any(test, feature = "web-probes"))]
impl From<crate::TenantContext> for TenantContextProbeVo {
    fn from(value: crate::TenantContext) -> Self {
        Self {
            tenant_id: value.tenant_id,
            isolation_level: serde_json::to_string(&value.isolation_level)
                .unwrap_or_else(|_| "\"shared_row\"".to_string())
                .trim_matches('"')
                .to_string(),
            datasource_override: value.datasource_override,
            schema_override: value.schema_override,
        }
    }
}

#[cfg(any(test, feature = "web-probes"))]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TenantProbeRowVo {
    pub id: i64,
    pub tenant_id: String,
    pub payload: String,
}

#[cfg(any(test, feature = "web-probes"))]
impl TenantProbeRowVo {
    fn from_query_result(row: QueryResult) -> Result<Self, sea_orm::DbErr> {
        Ok(Self {
            id: row.try_get("", "id")?,
            tenant_id: row.try_get("", "tenant_id")?,
            payload: row.try_get("", "payload")?,
        })
    }
}

#[cfg(any(test, feature = "web-probes"))]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TenantProbeRowsVo {
    pub tenant_id: String,
    pub row_count: usize,
    pub rows: Vec<TenantProbeRowVo>,
}

#[cfg(any(test, feature = "web-probes"))]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct IsolatedTenantProbeRowVo {
    pub id: i64,
    pub payload: String,
}

#[cfg(any(test, feature = "web-probes"))]
impl IsolatedTenantProbeRowVo {
    fn from_query_result(row: QueryResult) -> Result<Self, sea_orm::DbErr> {
        Ok(Self {
            id: row.try_get("", "id")?,
            payload: row.try_get("", "payload")?,
        })
    }
}

#[cfg(any(test, feature = "web-probes"))]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct IsolatedTenantProbeRowsVo {
    pub tenant_id: String,
    pub row_count: usize,
    pub rows: Vec<IsolatedTenantProbeRowVo>,
}

#[cfg(any(test, feature = "web-probes"))]
#[get_api("/internal/sharding/tenant-context")]
pub async fn tenant_context(CurrentTenant(tenant): CurrentTenant) -> Json<TenantContextProbeVo> {
    Json(TenantContextProbeVo::from(tenant))
}

#[cfg(any(test, feature = "web-probes"))]
#[get_api("/internal/sharding/probe/rows")]
pub async fn tenant_probe_rows(
    CurrentTenant(tenant): CurrentTenant,
    Component(sharding): Component<ShardingConnection>,
) -> Result<Json<TenantProbeRowsVo>, WebError> {
    let rows = sharding
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            SHARED_TENANT_PROBE_SQL,
        ))
        .await
        .map_err(internal_server_error)?;

    let rows = rows
        .into_iter()
        .map(TenantProbeRowVo::from_query_result)
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal_server_error)?;

    Ok(Json(TenantProbeRowsVo {
        tenant_id: tenant.tenant_id,
        row_count: rows.len(),
        rows,
    }))
}

#[cfg(any(test, feature = "web-probes"))]
#[get_api("/internal/sharding/probe/isolated-rows")]
pub async fn isolated_tenant_probe_rows(
    CurrentTenant(tenant): CurrentTenant,
    Component(sharding): Component<ShardingConnection>,
) -> Result<Json<IsolatedTenantProbeRowsVo>, WebError> {
    let rows = sharding
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            ISOLATED_TENANT_PROBE_SQL,
        ))
        .await
        .map_err(internal_server_error)?;

    let rows = rows
        .into_iter()
        .map(IsolatedTenantProbeRowVo::from_query_result)
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal_server_error)?;

    Ok(Json(IsolatedTenantProbeRowsVo {
        tenant_id: tenant.tenant_id,
        row_count: rows.len(),
        rows,
    }))
}

#[cfg(any(test, feature = "web-probes"))]
fn internal_server_error(error: impl std::fmt::Display) -> WebError {
    KnownWebError::internal_server_error(error.to_string()).into()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::Value;
    use summer::App;
    use summer::config::ConfigRegistry;
    use summer::plugin::MutableComponentRegistry;
    use summer_web::AppState;
    use summer_web::axum::{
        Extension, Json, Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        routing::get,
    };
    use summer_web::handler::auto_router;
    use tower::util::ServiceExt;

    use super::{IsolatedTenantProbeRowsVo, TenantContextProbeVo, TenantProbeRowsVo};
    use crate::{
        ShardingConnection, SummerShardingConfig,
        web::{CurrentTenant, TenantContextLayer},
    };

    async fn probe_handler(current_tenant: CurrentTenant) -> Json<TenantContextProbeVo> {
        Json(TenantContextProbeVo::from(current_tenant.0))
    }

    #[tokio::test]
    async fn tenant_context_route_returns_request_header_tenant() {
        let app = Router::new()
            .route("/internal/sharding/tenant-context", get(probe_handler))
            .layer(TenantContextLayer::from_header());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/internal/sharding/tenant-context")
                    .header("x-tenant-id", "T-REQ-ROUTE")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let payload: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload["tenant_id"], "T-REQ-ROUTE");
        assert_eq!(payload["isolation_level"], "shared_row");
    }

    fn e2e_database_url() -> String {
        std::env::var("SUMMER_SHARDING_E2E_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| {
                "postgres://admin:123456@localhost/summerrs-admin?options=-c%20TimeZone%3DAsia%2FShanghai"
                    .to_string()
            })
    }

    async fn build_real_probe_router() -> Router {
        let database_url = e2e_database_url();
        let config = format!(
            r#"
            [summer-sharding]
            enabled = true

            [summer-sharding.datasources.ds_test]
            uri = "{database_url}"
            schema = "test"
            role = "primary"

            [summer-sharding.tenant]
            enabled = true
            tenant_id_source = "header"
            default_isolation = "shared_row"

            [summer-sharding.tenant.row_level]
            column_name = "tenant_id"
            strategy = "sql_rewrite"
            "#
        );

        let mut builder = App::new();
        builder.use_config_str(&config);
        let sharding_config = builder
            .get_config::<SummerShardingConfig>()
            .expect("summer-sharding config");
        let tenant_id_source = sharding_config.tenant.tenant_id_source;
        let sharding = ShardingConnection::build(
            sharding_config
                .into_runtime_config()
                .expect("summer-sharding runtime config"),
        )
        .await
        .expect("build sharding connection");
        let metadata_connection = sea_orm::Database::connect(&database_url)
            .await
            .expect("connect metadata database");
        sharding
            .reload_tenant_metadata(&metadata_connection)
            .await
            .expect("reload tenant metadata");
        builder.add_component(sharding);
        let app: Arc<summer::app::App> = builder.build().await.expect("build test app");

        auto_router()
            .layer(Extension(AppState { app }))
            .layer(TenantContextLayer::from_source(tenant_id_source))
            .into()
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL test schema data"]
    async fn tenant_probe_route_filters_real_pg_rows_by_header() {
        let app = build_real_probe_router().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/internal/sharding/probe/rows")
                    .header("x-tenant-id", "T-E2E-A")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let payload: TenantProbeRowsVo = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload.tenant_id, "T-E2E-A");
        assert_eq!(payload.row_count, 2);
        assert_eq!(
            payload.rows,
            vec![
                super::TenantProbeRowVo {
                    id: payload.rows[0].id,
                    tenant_id: "T-E2E-A".to_string(),
                    payload: "alpha-1".to_string(),
                },
                super::TenantProbeRowVo {
                    id: payload.rows[1].id,
                    tenant_id: "T-E2E-A".to_string(),
                    payload: "alpha-2".to_string(),
                },
            ]
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/internal/sharding/probe/rows")
                    .header("x-tenant-id", "T-E2E-B")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let payload: TenantProbeRowsVo = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload.tenant_id, "T-E2E-B");
        assert_eq!(payload.row_count, 1);
        assert_eq!(payload.rows[0].tenant_id, "T-E2E-B");
        assert_eq!(payload.rows[0].payload, "beta-1");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL separate-schema seed data"]
    async fn tenant_probe_route_reads_rows_from_separate_schema_tenant() {
        let app = build_real_probe_router().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/internal/sharding/probe/isolated-rows")
                    .header("x-tenant-id", "T-SEED-SCHEMA")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let payload: IsolatedTenantProbeRowsVo = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload.tenant_id, "T-SEED-SCHEMA");
        assert_eq!(payload.row_count, 1);
        assert_eq!(payload.rows[0].payload, "schema-row-1");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL separate-table seed data"]
    async fn tenant_probe_route_reads_rows_from_separate_table_tenant() {
        let app = build_real_probe_router().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/internal/sharding/probe/isolated-rows")
                    .header("x-tenant-id", "T-SEED-TABLE")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let payload: IsolatedTenantProbeRowsVo = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload.tenant_id, "T-SEED-TABLE");
        assert_eq!(payload.row_count, 1);
        assert_eq!(payload.rows[0].payload, "table-row-1");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL separate-database seed data"]
    async fn tenant_probe_route_reads_rows_from_separate_database_tenant() {
        let app = build_real_probe_router().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/internal/sharding/probe/isolated-rows")
                    .header("x-tenant-id", "T-SEED-DB")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let payload: IsolatedTenantProbeRowsVo = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload.tenant_id, "T-SEED-DB");
        assert_eq!(payload.row_count, 1);
        assert_eq!(payload.rows[0].payload, "db-row-1");
    }
}
