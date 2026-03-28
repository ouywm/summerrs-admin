use std::{future::Future, pin::Pin};

use summer_auth::UserSession;
use summer_web::axum::{body::Body, extract::Request, response::Response};
use tower_layer::Layer;
use url::form_urlencoded;

use crate::{
    config::{TenantIdSource, TenantIsolationLevel},
    connector::ShardingConnection,
    tenant::TenantContext,
};

const TENANT_ID_HEADER: &str = "x-tenant-id";
const DEFAULT_TENANT_FIELD: &str = "tenant_id";

#[derive(Clone, Debug)]
pub struct TenantContextLayer {
    source: TenantIdSource,
    tenant_id_field: String,
    default_isolation: TenantIsolationLevel,
    sharding: Option<ShardingConnection>,
}

impl TenantContextLayer {
    pub fn new() -> Self {
        Self {
            source: TenantIdSource::RequestExtension,
            tenant_id_field: DEFAULT_TENANT_FIELD.to_string(),
            default_isolation: TenantIsolationLevel::SharedRow,
            sharding: None,
        }
    }

    pub fn from_source(source: TenantIdSource) -> Self {
        Self::from_source_and_field(source, DEFAULT_TENANT_FIELD)
    }

    pub fn from_source_and_field(
        source: TenantIdSource,
        tenant_id_field: impl Into<String>,
    ) -> Self {
        Self {
            source,
            tenant_id_field: tenant_id_field.into(),
            default_isolation: TenantIsolationLevel::SharedRow,
            sharding: None,
        }
    }

    pub fn from_header() -> Self {
        Self {
            source: TenantIdSource::Header,
            tenant_id_field: DEFAULT_TENANT_FIELD.to_string(),
            default_isolation: TenantIsolationLevel::SharedRow,
            sharding: None,
        }
    }

    pub fn with_default_isolation(mut self, default_isolation: TenantIsolationLevel) -> Self {
        self.default_isolation = default_isolation;
        self
    }

    pub fn with_sharding(mut self, sharding: ShardingConnection) -> Self {
        self.sharding = Some(sharding);
        self
    }
}

impl Default for TenantContextLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Clone> Layer<S> for TenantContextLayer {
    type Service = TenantContextMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TenantContextMiddleware {
            inner,
            source: self.source,
            tenant_id_field: self.tenant_id_field.clone(),
            default_isolation: self.default_isolation,
            sharding: self.sharding.clone(),
        }
    }
}

#[derive(Clone)]
pub struct TenantContextMiddleware<S> {
    inner: S,
    source: TenantIdSource,
    tenant_id_field: String,
    default_isolation: TenantIsolationLevel,
    sharding: Option<ShardingConnection>,
}

impl<S> tower_service::Service<Request> for TenantContextMiddleware<S>
where
    S: tower_service::Service<Request, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let tenant = resolve_tenant_context(
            &req,
            self.source,
            self.tenant_id_field.as_str(),
            self.default_isolation,
            self.sharding.as_ref(),
        );
        let request_scoped_sharding = tenant.as_ref().and_then(|tenant| {
            self.sharding
                .as_ref()
                .map(|sharding| sharding.with_tenant_context(tenant.clone()))
        });
        let mut inner = self.inner.clone();

        Box::pin(async move {
            if let Some(tenant) = tenant {
                req.extensions_mut().insert(tenant);
                if let Some(sharding) = request_scoped_sharding {
                    req.extensions_mut().insert(sharding);
                }
                inner.call(req).await
            } else {
                inner.call(req).await
            }
        })
    }
}

fn resolve_tenant_context(
    req: &Request,
    source: TenantIdSource,
    tenant_id_field: &str,
    default_isolation: TenantIsolationLevel,
    sharding: Option<&ShardingConnection>,
) -> Option<TenantContext> {
    let tenant = resolve_extension_tenant(req, default_isolation)
        .or_else(|| resolve_source_tenant(req, source, tenant_id_field, default_isolation))?;
    Some(match sharding {
        Some(sharding) => sharding.resolve_tenant_context(tenant),
        None => tenant,
    })
}

fn resolve_extension_tenant(
    req: &Request,
    default_isolation: TenantIsolationLevel,
) -> Option<TenantContext> {
    if let Some(tenant) = req.extensions().get::<TenantContext>() {
        return Some(tenant.clone());
    }

    if let Some(session) = req.extensions().get::<UserSession>()
        && let Some(tenant_id) = session.tenant_id.as_deref()
    {
        return Some(TenantContext::new(tenant_id, default_isolation));
    }

    None
}

fn resolve_source_tenant(
    req: &Request,
    source: TenantIdSource,
    tenant_id_field: &str,
    default_isolation: TenantIsolationLevel,
) -> Option<TenantContext> {
    match source {
        TenantIdSource::RequestExtension | TenantIdSource::Context => None,
        TenantIdSource::Header => req
            .headers()
            .get(TENANT_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|tenant_id| TenantContext::new(tenant_id, default_isolation)),
        TenantIdSource::JwtClaim => req
            .extensions()
            .get::<UserSession>()
            .and_then(|value| value.tenant_id.as_deref())
            .filter(|value| !value.is_empty())
            .map(|tenant_id| TenantContext::new(tenant_id, default_isolation)),
        TenantIdSource::QueryParam => req
            .uri()
            .query()
            .and_then(|query| {
                form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
                    (key == tenant_id_field && !value.is_empty()).then(|| value.into_owned())
                })
            })
            .map(|tenant_id| TenantContext::new(tenant_id, default_isolation)),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        future::Future,
        pin::Pin,
        sync::Arc,
        task::{Context, Poll},
    };

    use futures::executor::block_on;
    use parking_lot::Mutex;
    use sea_orm::{ConnectionTrait, DbBackend, MockDatabase, Statement};
    use summer_auth::{AdminProfile, DeviceType, LoginId, UserProfile, UserSession};
    use summer_web::axum::{
        body::Body,
        extract::Request,
        http::{Request as HttpRequest, Response as HttpResponse},
        response::Response,
    };
    use tower_layer::Layer;
    use tower_service::Service;

    use crate::{
        TenantIsolationLevel,
        config::{ShardingConfig, TenantIdSource},
        connector::ShardingConnection,
        datasource::DataSourcePool,
        tenant::TenantContext,
        web::TenantContextLayer,
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CapturedTenantState {
        extension_tenant: Option<TenantContext>,
        extension_sharding: bool,
    }

    #[derive(Clone)]
    struct CaptureTenantService {
        captured: Arc<Mutex<Vec<CapturedTenantState>>>,
    }

    impl CaptureTenantService {
        fn new(captured: Arc<Mutex<Vec<CapturedTenantState>>>) -> Self {
            Self { captured }
        }
    }

    impl tower_service::Service<Request> for CaptureTenantService {
        type Response = Response<Body>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let captured = self.captured.clone();
            Box::pin(async move {
                captured.lock().push(CapturedTenantState {
                    extension_tenant: req.extensions().get::<TenantContext>().cloned(),
                    extension_sharding: req.extensions().get::<ShardingConnection>().is_some(),
                });
                Ok(HttpResponse::new(Body::empty()))
            })
        }
    }

    #[derive(Clone)]
    struct ShardingProbeService;

    impl tower_service::Service<Request> for ShardingProbeService {
        type Response = Response<Body>;
        type Error = sea_orm::DbErr;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request) -> Self::Future {
            Box::pin(async move {
                let sharding = req
                    .extensions()
                    .get::<ShardingConnection>()
                    .cloned()
                    .expect("request scoped sharding connection");
                sharding
                    .query_all_raw(Statement::from_string(
                        DbBackend::Postgres,
                        "SELECT id FROM ai.log WHERE status = 1",
                    ))
                    .await?;
                Ok(HttpResponse::new(Body::empty()))
            })
        }
    }

    fn request() -> Request {
        HttpRequest::builder()
            .uri("/tenant")
            .body(Body::empty())
            .expect("request")
    }

    fn request_with_tenant_header(tenant_id: &str) -> Request {
        HttpRequest::builder()
            .uri("/tenant")
            .header("x-tenant-id", tenant_id)
            .body(Body::empty())
            .expect("request")
    }

    fn request_with_query(query: &str) -> Request {
        HttpRequest::builder()
            .uri(format!("/tenant?{query}"))
            .body(Body::empty())
            .expect("request")
    }

    #[test]
    fn tenant_context_layer_inserts_tenant_into_request_extensions() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service =
            TenantContextLayer::new().layer(CaptureTenantService::new(captured.clone()));

        block_on(service.call(request())).expect("call");

        let records = captured.lock();
        assert_eq!(records.len(), 1);
        assert!(records[0].extension_tenant.is_none());
        assert!(!records[0].extension_sharding);
    }

    #[test]
    fn tenant_context_layer_reads_tenant_header_when_present() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service =
            TenantContextLayer::from_header().layer(CaptureTenantService::new(captured.clone()));

        block_on(service.call(request_with_tenant_header("T-REQ-001"))).expect("call");

        let records = captured.lock();
        assert_eq!(
            records[0]
                .extension_tenant
                .clone()
                .expect("tenant in extensions")
                .tenant_id,
            "T-REQ-001"
        );
        assert!(!records[0].extension_sharding);
    }

    #[test]
    fn tenant_context_layer_reads_existing_request_extension_tenant_by_default() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service =
            TenantContextLayer::new().layer(CaptureTenantService::new(captured.clone()));
        let mut req = request();
        req.extensions_mut().insert(TenantContext::new(
            "T-EXT-001",
            TenantIsolationLevel::SharedRow,
        ));

        block_on(service.call(req)).expect("call");

        let records = captured.lock();
        assert_eq!(
            records[0]
                .extension_tenant
                .clone()
                .expect("tenant in extensions")
                .tenant_id,
            "T-EXT-001"
        );
        assert!(!records[0].extension_sharding);
    }

    #[test]
    fn tenant_context_layer_reads_tenant_from_auth_session_extension() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service =
            TenantContextLayer::new().layer(CaptureTenantService::new(captured.clone()));
        let mut req = request();
        req.extensions_mut().insert(UserSession {
            login_id: LoginId::admin(1),
            device: DeviceType::Web,
            tenant_id: Some("T-AUTH-001".to_string()),
            profile: UserProfile::Admin(AdminProfile {
                user_name: "admin".to_string(),
                nick_name: "Admin".to_string(),
                roles: vec!["admin".to_string()],
                permissions: vec![],
            }),
        });

        block_on(service.call(req)).expect("call");

        let records = captured.lock();
        assert_eq!(
            records[0]
                .extension_tenant
                .clone()
                .expect("tenant in extensions")
                .tenant_id,
            "T-AUTH-001"
        );
        assert!(!records[0].extension_sharding);
    }

    #[test]
    fn tenant_context_layer_reads_tenant_from_auth_session_when_jwt_claim_mode_is_configured() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service = TenantContextLayer::from_source(TenantIdSource::JwtClaim)
            .layer(CaptureTenantService::new(captured.clone()));
        let mut req = request();
        req.extensions_mut().insert(UserSession {
            login_id: LoginId::admin(9),
            device: DeviceType::Web,
            tenant_id: Some("T-CLAIM-001".to_string()),
            profile: UserProfile::Admin(AdminProfile {
                user_name: "admin".to_string(),
                nick_name: "Admin".to_string(),
                roles: vec!["admin".to_string()],
                permissions: vec![],
            }),
        });

        block_on(service.call(req)).expect("call");

        let records = captured.lock();
        assert_eq!(
            records[0]
                .extension_tenant
                .clone()
                .expect("tenant in extensions")
                .tenant_id,
            "T-CLAIM-001"
        );
        assert!(!records[0].extension_sharding);
    }

    #[test]
    fn tenant_context_layer_context_mode_ignores_header() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service =
            TenantContextLayer::new().layer(CaptureTenantService::new(captured.clone()));

        block_on(service.call(request_with_tenant_header("T-SHOULD-BE-IGNORED"))).expect("call");

        let records = captured.lock();
        assert!(records[0].extension_tenant.is_none());
        assert!(!records[0].extension_sharding);
    }

    #[test]
    fn tenant_context_layer_keeps_requests_without_header_unscoped() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service =
            TenantContextLayer::new().layer(CaptureTenantService::new(captured.clone()));

        block_on(service.call(request())).expect("first call");
        block_on(service.call(request())).expect("second call");

        let records = captured.lock();
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|record| record.extension_tenant.is_none())
        );
        assert!(records.iter().all(|record| !record.extension_sharding));
    }

    #[test]
    fn tenant_context_layer_reads_query_param_from_configured_field() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service =
            TenantContextLayer::from_source_and_field(TenantIdSource::QueryParam, "tenant")
                .layer(CaptureTenantService::new(captured.clone()));

        block_on(service.call(request_with_query("tenant=T-QUERY-001"))).expect("call");

        let records = captured.lock();
        assert_eq!(
            records[0]
                .extension_tenant
                .clone()
                .expect("tenant in extensions")
                .tenant_id,
            "T-QUERY-001"
        );
        assert!(!records[0].extension_sharding);
    }

    #[test]
    fn tenant_context_layer_resolves_header_tenant_with_metadata_snapshot() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let config = Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [tenant]
                enabled = true
                tenant_id_source = "header"
                default_isolation = "shared_row"
                "#,
            )
            .expect("config"),
        );
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([(
                "ds_ai".to_string(),
                MockDatabase::new(DbBackend::Postgres).into_connection(),
            )]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");
        sharding
            .tenant_metadata_store()
            .upsert(crate::TenantMetadataRecord {
                tenant_id: "T-SCHEMA-001".to_string(),
                isolation_level: TenantIsolationLevel::SeparateSchema,
                status: Some("active".to_string()),
                schema_name: Some("tenant_schema_001".to_string()),
                datasource_name: Some("ds_schema_001".to_string()),
                db_uri: None,
                db_max_conns: None,
            });

        let mut service = TenantContextLayer::from_header()
            .with_sharding(sharding)
            .layer(CaptureTenantService::new(captured.clone()));

        block_on(service.call(request_with_tenant_header("T-SCHEMA-001"))).expect("call");

        let records = captured.lock();
        let tenant = records[0]
            .extension_tenant
            .clone()
            .expect("tenant in extensions");
        assert_eq!(tenant.tenant_id, "T-SCHEMA-001");
        assert_eq!(tenant.isolation_level, TenantIsolationLevel::SeparateSchema);
        assert_eq!(tenant.schema_override.as_deref(), Some("tenant_schema_001"));
        assert_eq!(tenant.datasource_override.as_deref(), Some("ds_schema_001"));
        assert!(records[0].extension_sharding);
    }

    #[test]
    fn tenant_http_scope_drives_shared_row_sql_rewrite() {
        let config = Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [tenant]
                enabled = true
                tenant_id_source = "header"
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#,
            )
            .expect("config"),
        );
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, sea_orm::Value>>::new()])
            .into_connection();
        let log_connection = connection.clone();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([("ds_ai".to_string(), connection)]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("connection");

        let mut service = TenantContextLayer::from_header()
            .with_sharding(sharding)
            .layer(ShardingProbeService);

        block_on(service.call(request_with_tenant_header("T-SQL-001"))).expect("call");

        let logs = log_connection.into_transaction_log();
        assert_eq!(logs.len(), 1);
        assert!(
            logs[0].statements()[0]
                .sql
                .contains("tenant_id = 'T-SQL-001'")
        );
    }

    #[test]
    fn tenant_context_layer_new_does_not_insert_tenant_without_header() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut service =
            TenantContextLayer::new().layer(CaptureTenantService::new(captured.clone()));

        block_on(service.call(request())).expect("call");

        let records = captured.lock();
        assert!(records[0].extension_tenant.is_none());
    }
}
