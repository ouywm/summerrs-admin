use summer_admin_macros::log;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_system_model::dto::sys_tenant::{
    ChangeTenantStatusDto, CreateTenantDto, ProvisionTenantDto, SaveTenantDatasourceDto,
    SaveTenantMembershipDto, TenantQueryDto, UpdateTenantDto,
};
use summer_system_model::vo::sys_tenant::{
    TenantDatasourceVo, TenantDetailVo, TenantMembershipVo, TenantProvisionResultVo,
    TenantRouteStateVo, TenantRuntimeDatasourceVo, TenantRuntimeRefreshVo, TenantVo,
};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{get_api, post_api, put_api};

use crate::service::sys_tenant_service::SysTenantService;

#[log(module = "租户管理", action = "查询租户列表", biz_type = Query)]
#[get_api("/tenant/list")]
pub async fn list_tenants(
    Component(svc): Component<SysTenantService>,
    Query(query): Query<TenantQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<TenantVo>>> {
    let page = svc.list_tenants(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "租户管理", action = "查询租户详情", biz_type = Query)]
#[get_api("/tenant/{tenant_id}")]
pub async fn tenant_detail(
    Component(svc): Component<SysTenantService>,
    Path(tenant_id): Path<String>,
) -> ApiResult<Json<TenantDetailVo>> {
    let detail = svc.get_tenant_detail(tenant_id.as_str()).await?;
    Ok(Json(detail))
}

#[log(module = "租户管理", action = "创建租户", biz_type = Create)]
#[post_api("/tenant")]
pub async fn create_tenant(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysTenantService>,
    ValidatedJson(dto): ValidatedJson<CreateTenantDto>,
) -> ApiResult<()> {
    svc.create_tenant(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "租户管理", action = "更新租户", biz_type = Update)]
#[put_api("/tenant/{tenant_id}")]
pub async fn update_tenant(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysTenantService>,
    Path(tenant_id): Path<String>,
    ValidatedJson(dto): ValidatedJson<UpdateTenantDto>,
) -> ApiResult<()> {
    svc.update_tenant(tenant_id.as_str(), dto, &profile.nick_name)
        .await?;
    Ok(())
}

#[log(module = "租户管理", action = "切换租户状态", biz_type = Update)]
#[put_api("/tenant/{tenant_id}/status")]
pub async fn change_tenant_status(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysTenantService>,
    Path(tenant_id): Path<String>,
    ValidatedJson(dto): ValidatedJson<ChangeTenantStatusDto>,
) -> ApiResult<()> {
    svc.change_tenant_status(tenant_id.as_str(), dto, &profile.nick_name)
        .await?;
    Ok(())
}

#[log(
    module = "租户管理",
    action = "保存租户数据源",
    biz_type = Update,
    save_params = false
)]
#[put_api("/tenant/{tenant_id}/datasource")]
pub async fn save_tenant_datasource(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysTenantService>,
    Path(tenant_id): Path<String>,
    ValidatedJson(dto): ValidatedJson<SaveTenantDatasourceDto>,
) -> ApiResult<Json<TenantDatasourceVo>> {
    let datasource = svc
        .save_tenant_datasource(tenant_id.as_str(), dto, &profile.nick_name)
        .await?;
    Ok(Json(datasource))
}

#[log(module = "租户管理", action = "查询租户成员", biz_type = Query)]
#[get_api("/tenant/{tenant_id}/members")]
pub async fn list_tenant_members(
    Component(svc): Component<SysTenantService>,
    Path(tenant_id): Path<String>,
) -> ApiResult<Json<Vec<TenantMembershipVo>>> {
    let items = svc.list_tenant_members(tenant_id.as_str()).await?;
    Ok(Json(items))
}

#[log(module = "租户管理", action = "保存租户成员", biz_type = Update)]
#[put_api("/tenant/{tenant_id}/members")]
pub async fn save_tenant_membership(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysTenantService>,
    Path(tenant_id): Path<String>,
    ValidatedJson(dto): ValidatedJson<SaveTenantMembershipDto>,
) -> ApiResult<()> {
    svc.save_tenant_membership(tenant_id.as_str(), dto, &profile.nick_name)
        .await?;
    Ok(())
}

#[log(
    module = "租户管理",
    action = "租户资源开通",
    biz_type = Update,
    save_params = false
)]
#[post_api("/tenant/{tenant_id}/provision")]
pub async fn provision_tenant(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysTenantService>,
    Path(tenant_id): Path<String>,
    ValidatedJson(dto): ValidatedJson<ProvisionTenantDto>,
) -> ApiResult<Json<TenantProvisionResultVo>> {
    let result = svc
        .provision_tenant(tenant_id.as_str(), dto, &profile.nick_name)
        .await?;
    Ok(Json(result))
}

#[log(module = "租户管理", action = "刷新分片运行时", biz_type = Update)]
#[post_api("/tenant/runtime/refresh")]
pub async fn refresh_tenant_runtime(
    Component(svc): Component<SysTenantService>,
) -> ApiResult<Json<TenantRuntimeRefreshVo>> {
    let result = svc.refresh_runtime_metadata().await?;
    Ok(Json(result))
}

#[log(module = "租户管理", action = "查询数据源健康状态", biz_type = Query)]
#[get_api("/tenant/runtime/health")]
pub async fn tenant_runtime_health(
    Component(svc): Component<SysTenantService>,
) -> ApiResult<Json<Vec<TenantRuntimeDatasourceVo>>> {
    let items = svc.runtime_health().await?;
    Ok(Json(items))
}

#[log(module = "租户管理", action = "查询路由状态", biz_type = Query)]
#[get_api("/tenant/runtime/routes")]
pub async fn tenant_runtime_routes(
    Component(svc): Component<SysTenantService>,
) -> ApiResult<Json<Vec<TenantRouteStateVo>>> {
    let items = svc.runtime_routes().await?;
    Ok(Json(items))
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(list_tenants)
        .typed_route(tenant_detail)
        .typed_route(create_tenant)
        .typed_route(update_tenant)
        .typed_route(change_tenant_status)
        .typed_route(save_tenant_datasource)
        .typed_route(list_tenant_members)
        .typed_route(save_tenant_membership)
        .typed_route(provision_tenant)
        .typed_route(refresh_tenant_runtime)
        .typed_route(tenant_runtime_health)
        .typed_route(tenant_runtime_routes)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, OnceLock};

    use sea_orm::{
        ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, EntityTrait, QueryFilter, Set,
    };
    use summer::App;
    use summer::plugin::MutableComponentRegistry;
    use summer_auth::config::AuthConfig;
    use summer_auth::storage::memory::MemoryStorage;
    use summer_auth::{DeviceType, LoginId, UserProfile, UserSession};
    use summer_plugins::{
        BackgroundTaskPlugin, Ip2RegionPlugin, LogBatchCollectorPlugin, S3Plugin,
    };
    use summer_redis::redis::Client as RedisClient;
    use summer_sea_orm::SeaOrmPlugin;
    use summer_sharding::SummerShardingPlugin;
    use summer_sharding::algorithm::normalize_tenant_suffix;
    use summer_system_model::entity::{sys_tenant, sys_tenant_datasource, sys_user};
    use summer_web::axum::{
        Extension,
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode},
    };
    use summer_web::handler::TypeRouter;
    use summer_web::socketioxide::SocketIo;
    use summer_web::{AppState, Router};
    use tokio::sync::Mutex;
    use tower::util::ServiceExt;
    use url::Url;

    use super::{
        change_tenant_status, create_tenant, list_tenant_members, list_tenants, provision_tenant,
        refresh_tenant_runtime, save_tenant_datasource, save_tenant_membership, tenant_detail,
        tenant_runtime_health, tenant_runtime_routes, update_tenant,
    };

    const TEST_REDIS_URL: &str = "redis://127.0.0.1/";
    const TEST_SOCKET_NAMESPACE: &str = "/summer-admin";
    const TEST_SOCKET_REDIS_PREFIX: &str = "summerrs:test:socket";
    const TEST_SOCKET_TTL_SECONDS: u64 = 86_400;
    const TEST_IP2REGION_V4_PATH: &str =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/ip2region_v4.xdb");

    fn schema_prepare_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn e2e_database_url() -> String {
        std::env::var("SUMMER_SHARDING_E2E_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| {
                "postgres://admin:123456@localhost/summerrs-admin?options=-c%20TimeZone%3DAsia%2FShanghai"
                    .to_string()
            })
    }

    fn admin_session() -> UserSession {
        UserSession {
            login_id: LoginId::new(1),
            device: DeviceType::Web,
            tenant_id: None,
            profile: UserProfile {
                user_name: "admin".to_string(),
                nick_name: "Admin".to_string(),
                roles: vec!["admin".to_string()],
                permissions: vec!["system:tenant:*".to_string()],
            },
        }
    }

    fn test_auth_config() -> AuthConfig {
        serde_json::from_str(
            r#"{
                "token_name": "Authorization",
                "access_timeout": 3600,
                "refresh_timeout": 86400,
                "concurrent_login": true,
                "max_devices": 5,
                "qr_code_timeout": 300,
                "jwt_secret": "test-jwt-secret-key-for-sys-tenant-http"
            }"#,
        )
        .expect("parse auth config")
    }

    fn build_test_config(db_url: &str) -> String {
        format!(
            r#"
            [sea-orm]
            enable_logging = true
            uri = "{db_url}"

            [summer-sharding]
            enabled = true

            [summer-sharding.datasources.ds_test]
            uri = "{db_url}"
            schema = "test"
            role = "primary"

            [summer-sharding.tenant]
            enabled = true
            tenant_id_source = "request_extension"
            default_isolation = "shared_row"

            [summer-sharding.tenant.row_level]
            column_name = "tenant_id"
            strategy = "sql_rewrite"

            [sea-orm-web]
            default_page_size = 20
            max_page_size = 2000
            one_indexed = true

            [ip2region]
            ipv4_db_path = "{TEST_IP2REGION_V4_PATH}"

            [socket_io]
            default_namespace = "{TEST_SOCKET_NAMESPACE}"

            [socket-gateway]
            redis_prefix = "{TEST_SOCKET_REDIS_PREFIX}"
            session_ttl_seconds = {TEST_SOCKET_TTL_SECONDS}

            [s3]
            access_key = "test-access-key"
            secret_key = "test-secret-key"
            bucket = "summer-admin"
            endpoint = "http://localhost:9000"
            region = "us-east-1"
            "#
        )
    }

    async fn response_json(response: summer_web::axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        serde_json::from_slice(&body).expect("response json")
    }

    async fn build_test_router(db_url: &str) -> Router {
        ensure_sharding_metadata_runtime_schema(db_url)
            .await
            .expect("prepare sharding metadata schema");

        let mut builder = App::new();
        builder.use_config_str(&build_test_config(db_url));
        builder.add_plugin(SeaOrmPlugin);
        builder.add_plugin(SummerShardingPlugin);
        builder.add_plugin(Ip2RegionPlugin);
        builder.add_plugin(S3Plugin);
        builder.add_plugin(BackgroundTaskPlugin);
        builder.add_plugin(LogBatchCollectorPlugin);

        let redis = RedisClient::open(TEST_REDIS_URL)
            .expect("create redis client")
            .get_connection_manager()
            .await
            .expect("connect redis");
        let session_manager =
            summer_auth::SessionManager::new(Arc::new(MemoryStorage::new()), test_auth_config());
        let (_socket_service, socket_io) = SocketIo::new_svc();

        builder.add_component(redis);
        builder.add_component(session_manager);
        builder.add_component(socket_io);

        let app: Arc<summer::app::App> = builder.build().await.expect("build test app");

        Router::new()
            .typed_route(list_tenants)
            .typed_route(create_tenant)
            .typed_route(tenant_detail)
            .typed_route(update_tenant)
            .typed_route(change_tenant_status)
            .typed_route(save_tenant_datasource)
            .typed_route(list_tenant_members)
            .typed_route(save_tenant_membership)
            .typed_route(provision_tenant)
            .typed_route(refresh_tenant_runtime)
            .typed_route(tenant_runtime_health)
            .typed_route(tenant_runtime_routes)
            .layer(Extension(admin_session()))
            .layer(Extension(AppState { app }))
    }

    async fn ensure_sharding_metadata_runtime_schema(db_url: &str) -> Result<(), sea_orm::DbErr> {
        let _guard = schema_prepare_lock().lock().await;
        let db = Database::connect(db_url).await?;
        db.execute_unprepared(
            r#"
            CREATE SCHEMA IF NOT EXISTS sys;

            CREATE TABLE IF NOT EXISTS sys.tenant_datasource (
                id BIGSERIAL PRIMARY KEY,
                tenant_id VARCHAR(64) NOT NULL,
                isolation_level SMALLINT NOT NULL DEFAULT 1,
                status VARCHAR(32) NOT NULL DEFAULT 'active',
                schema_name VARCHAR(128),
                datasource_name VARCHAR(128),
                db_uri VARCHAR(1024),
                db_enable_logging BOOLEAN,
                db_min_conns INT,
                db_max_conns INT,
                db_connect_timeout_ms BIGINT,
                db_idle_timeout_ms BIGINT,
                db_acquire_timeout_ms BIGINT,
                db_test_before_acquire BOOLEAN,
                readonly_config JSONB NOT NULL DEFAULT '{}'::jsonb,
                extra_config JSONB NOT NULL DEFAULT '{}'::jsonb,
                last_sync_time TIMESTAMP,
                remark VARCHAR(500) NOT NULL DEFAULT '',
                create_by VARCHAR(64) NOT NULL DEFAULT '',
                create_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                update_by VARCHAR(64) NOT NULL DEFAULT '',
                update_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE UNIQUE INDEX IF NOT EXISTS uk_sys_tenant_datasource_tenant_id
                ON sys.tenant_datasource (tenant_id);

            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS db_enable_logging BOOLEAN;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS db_min_conns INT;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS db_max_conns INT;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS db_connect_timeout_ms BIGINT;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS db_idle_timeout_ms BIGINT;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS db_acquire_timeout_ms BIGINT;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS db_test_before_acquire BOOLEAN;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS readonly_config JSONB NOT NULL DEFAULT '{}'::jsonb;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS extra_config JSONB NOT NULL DEFAULT '{}'::jsonb;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS last_sync_time TIMESTAMP;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS remark VARCHAR(500) NOT NULL DEFAULT '';
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS create_by VARCHAR(64) NOT NULL DEFAULT '';
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS create_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP;
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS update_by VARCHAR(64) NOT NULL DEFAULT '';
            ALTER TABLE sys.tenant_datasource ADD COLUMN IF NOT EXISTS update_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP;
            "#,
        )
        .await?;
        Ok(())
    }

    async fn cleanup_tenant(db_url: &str, tenant_id: &str, provision_schema: Option<&str>) {
        let db = Database::connect(db_url).await.expect("connect database");
        if let Some(schema) = provision_schema {
            db.execute_unprepared(format!("DROP SCHEMA IF EXISTS {schema} CASCADE").as_str())
                .await
                .ok();
        }
        let _ = sys_tenant_datasource::Entity::delete_many()
            .filter(sys_tenant_datasource::Column::TenantId.eq(tenant_id))
            .exec(&db)
            .await;
        let _ = sys_tenant::Entity::delete_many()
            .filter(sys_tenant::Column::TenantId.eq(tenant_id))
            .exec(&db)
            .await;
    }

    async fn cleanup_user(db_url: &str, user_id: i64) {
        let db = Database::connect(db_url).await.expect("connect database");
        let _ = sys_user::Entity::delete_by_id(user_id).exec(&db).await;
    }

    fn build_database_url(base_db_url: &str, database_name: &str) -> String {
        let mut url = Url::parse(base_db_url).expect("parse base database url");
        url.set_path(database_name);
        url.to_string()
    }

    async fn database_exists(db_url: &str, database_name: &str) -> bool {
        let db = Database::connect(db_url).await.expect("connect database");
        let row = db
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Postgres,
                format!(
                    "SELECT datname FROM pg_database WHERE datname = '{}'",
                    database_name.replace('\'', "''")
                ),
            ))
            .await
            .expect("query pg_database");
        row.is_some()
    }

    async fn cleanup_database(db_url: &str, database_name: &str) {
        let db = Database::connect(db_url).await.expect("connect database");
        let terminate_sql = format!(
            "SELECT pg_terminate_backend(pid) \
             FROM pg_stat_activity \
             WHERE datname = '{}' AND pid <> pg_backend_pid()",
            database_name.replace('\'', "''")
        );
        db.execute_unprepared(&terminate_sql)
            .await
            .expect("terminate database backends");
        db.execute_unprepared(&format!("DROP DATABASE IF EXISTS \"{database_name}\""))
            .await
            .expect("drop database");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL"]
    async fn tenant_http_create_and_list_roundtrip() {
        let db_url = e2e_database_url();
        let tenant_id = format!("T-HTTP-{}", chrono::Utc::now().timestamp_millis());
        cleanup_tenant(&db_url, tenant_id.as_str(), None).await;
        let app = build_test_router(&db_url).await;

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tenant")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "tenantId": tenant_id,
                            "tenantName": "HTTP Tenant",
                            "defaultIsolationLevel": 1,
                            "status": 1,
                            "contactName": "Tester"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("create response");
        assert_eq!(create_response.status(), StatusCode::OK);

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/tenant/list?page=1&size=20&tenantId={tenant_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("list response");
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_payload = response_json(list_response).await;
        assert_eq!(
            list_payload["content"][0]["tenantId"], tenant_id,
            "unexpected list payload: {list_payload}"
        );
        assert_eq!(
            list_payload["content"][0]["tenantName"], "HTTP Tenant",
            "unexpected list payload: {list_payload}"
        );
        assert_eq!(
            list_payload["content"][0]["defaultIsolationLevel"], 1,
            "unexpected list payload: {list_payload}"
        );
        assert_eq!(
            list_payload["content"][0]["datasource"]["status"], "active",
            "unexpected list payload: {list_payload}"
        );

        let detail_response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/tenant/{tenant_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("detail response");
        assert_eq!(detail_response.status(), StatusCode::OK);
        let detail_payload = response_json(detail_response).await;
        assert_eq!(
            detail_payload["tenantId"], tenant_id,
            "unexpected detail payload: {detail_payload}"
        );
        assert_eq!(
            detail_payload["datasource"]["status"], "active",
            "unexpected detail payload: {detail_payload}"
        );

        cleanup_tenant(&db_url, tenant_id.as_str(), None).await;
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL"]
    async fn tenant_http_provision_separate_schema_roundtrip() {
        let db_url = e2e_database_url();
        let suffix = chrono::Utc::now().timestamp_millis();
        let tenant_id = format!("T-PROVISION-{suffix}");
        let schema_name = format!("tenant_http_{suffix}");
        let base_table = format!("test.tenant_provision_base_{suffix}");
        cleanup_tenant(&db_url, tenant_id.as_str(), Some(schema_name.as_str())).await;

        let db = Database::connect(&db_url).await.expect("connect database");
        db.execute_unprepared("CREATE SCHEMA IF NOT EXISTS test")
            .await
            .expect("create test schema");
        db.execute_unprepared(
            format!(
                "CREATE TABLE IF NOT EXISTS {base_table} (id BIGSERIAL PRIMARY KEY, tenant_id VARCHAR(64) NOT NULL DEFAULT '', payload VARCHAR(128) NOT NULL DEFAULT '')"
            )
            .as_str(),
        )
        .await
        .expect("create base table");

        let app = build_test_router(&db_url).await;

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tenant")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "tenantId": tenant_id,
                            "tenantName": "Provision Tenant",
                            "defaultIsolationLevel": 1,
                            "status": 3
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("create response");
        assert_eq!(create_response.status(), StatusCode::OK);

        let provision_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/tenant/{tenant_id}/provision"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "isolationLevel": 3,
                            "schemaName": schema_name,
                            "baseTables": [base_table]
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("provision response");
        assert_eq!(provision_response.status(), StatusCode::OK);
        let provision_payload = response_json(provision_response).await;
        assert_eq!(provision_payload["tenantId"], tenant_id);
        assert_eq!(provision_payload["isolationLevel"], 3);
        assert_eq!(provision_payload["datasource"]["schemaName"], schema_name);

        let exists = db
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Postgres,
                format!(
                    "SELECT to_regclass('{schema_name}.{}')::text AS regclass",
                    base_table.rsplit('.').next().expect("table name")
                ),
            ))
            .await
            .expect("query regclass")
            .expect("row");
        let regclass: Option<String> = exists.try_get("", "regclass").expect("regclass");
        assert!(regclass.is_some());

        let tenant = sys_tenant::Entity::find()
            .filter(sys_tenant::Column::TenantId.eq(tenant_id.as_str()))
            .one(&db)
            .await
            .expect("query tenant")
            .expect("tenant");
        assert_eq!(tenant.status, sys_tenant::TenantStatus::Enabled);

        let datasource = sys_tenant_datasource::Entity::find()
            .filter(sys_tenant_datasource::Column::TenantId.eq(tenant_id.as_str()))
            .one(&db)
            .await
            .expect("query datasource")
            .expect("datasource");
        assert_eq!(
            datasource.status,
            sys_tenant_datasource::TenantDatasourceStatus::Active
        );
        assert_eq!(
            datasource.schema_name.as_deref(),
            Some(schema_name.as_str())
        );

        db.execute_unprepared(format!("DROP TABLE IF EXISTS {base_table}").as_str())
            .await
            .ok();
        cleanup_tenant(&db_url, tenant_id.as_str(), Some(schema_name.as_str())).await;
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL"]
    async fn tenant_http_provision_separate_table_roundtrip() {
        let db_url = e2e_database_url();
        let suffix = chrono::Utc::now().timestamp_millis();
        let tenant_id = format!("T-PROVISION-TABLE-{suffix}");
        let base_table = format!("test.tenant_table_base_{suffix}");
        let tenant_suffix = normalize_tenant_suffix(tenant_id.as_str());
        let provisioned_table = format!("{base_table}_{tenant_suffix}");
        cleanup_tenant(&db_url, tenant_id.as_str(), None).await;

        let db = Database::connect(&db_url).await.expect("connect database");
        db.execute_unprepared("CREATE SCHEMA IF NOT EXISTS test")
            .await
            .expect("create test schema");
        db.execute_unprepared(format!("DROP TABLE IF EXISTS {provisioned_table}").as_str())
            .await
            .ok();
        db.execute_unprepared(
            format!(
                "CREATE TABLE IF NOT EXISTS {base_table} (id BIGSERIAL PRIMARY KEY, tenant_id VARCHAR(64) NOT NULL DEFAULT '', payload VARCHAR(128) NOT NULL DEFAULT '')"
            )
            .as_str(),
        )
        .await
        .expect("create base table");

        let app = build_test_router(&db_url).await;

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tenant")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "tenantId": tenant_id,
                            "tenantName": "Provision Table Tenant",
                            "defaultIsolationLevel": 1,
                            "status": 3
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("create response");
        assert_eq!(create_response.status(), StatusCode::OK);

        let provision_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/tenant/{tenant_id}/provision"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "isolationLevel": 2,
                            "baseTables": [base_table]
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("provision response");
        assert_eq!(provision_response.status(), StatusCode::OK);
        let provision_payload = response_json(provision_response).await;
        assert_eq!(provision_payload["tenantId"], tenant_id);
        assert_eq!(provision_payload["isolationLevel"], 2);
        assert_eq!(provision_payload["datasource"]["isolationLevel"], 2);

        let exists = db
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Postgres,
                format!("SELECT to_regclass('{provisioned_table}')::text AS regclass"),
            ))
            .await
            .expect("query regclass")
            .expect("row");
        let regclass: Option<String> = exists.try_get("", "regclass").expect("regclass");
        assert_eq!(regclass.as_deref(), Some(provisioned_table.as_str()));

        let tenant = sys_tenant::Entity::find()
            .filter(sys_tenant::Column::TenantId.eq(tenant_id.as_str()))
            .one(&db)
            .await
            .expect("query tenant")
            .expect("tenant");
        assert_eq!(tenant.status, sys_tenant::TenantStatus::Enabled);
        assert_eq!(
            tenant.default_isolation_level,
            sys_tenant::TenantIsolationLevel::SeparateTable
        );

        let datasource = sys_tenant_datasource::Entity::find()
            .filter(sys_tenant_datasource::Column::TenantId.eq(tenant_id.as_str()))
            .one(&db)
            .await
            .expect("query datasource")
            .expect("datasource");
        assert_eq!(
            datasource.isolation_level,
            sys_tenant_datasource::TenantIsolationLevel::SeparateTable
        );
        assert_eq!(
            datasource.status,
            sys_tenant_datasource::TenantDatasourceStatus::Active
        );

        db.execute_unprepared(format!("DROP TABLE IF EXISTS {provisioned_table}").as_str())
            .await
            .ok();
        db.execute_unprepared(format!("DROP TABLE IF EXISTS {base_table}").as_str())
            .await
            .ok();
        cleanup_tenant(&db_url, tenant_id.as_str(), None).await;
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL"]
    async fn tenant_http_provision_separate_database_roundtrip() {
        let db_url = e2e_database_url();
        let suffix = chrono::Utc::now().timestamp_millis();
        let tenant_id = format!("T-PROVISION-DB-{suffix}");
        let database_name = format!("tenant_http_db_{suffix}");
        let datasource_name = format!("tenant_http_ds_{suffix}");
        let provision_db_url = build_database_url(&db_url, database_name.as_str());

        cleanup_tenant(&db_url, tenant_id.as_str(), None).await;
        if database_exists(&db_url, database_name.as_str()).await {
            cleanup_database(&db_url, database_name.as_str()).await;
        }

        let db = Database::connect(&db_url).await.expect("connect database");
        let app = build_test_router(&db_url).await;

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tenant")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "tenantId": tenant_id,
                            "tenantName": "Provision Database Tenant",
                            "defaultIsolationLevel": 1,
                            "status": 3
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("create response");
        assert_eq!(create_response.status(), StatusCode::OK);

        let provision_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/tenant/{tenant_id}/provision"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "isolationLevel": 4,
                            "datasourceName": datasource_name,
                            "dbUri": provision_db_url,
                            "dbMaxConns": 5
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("provision response");
        assert_eq!(provision_response.status(), StatusCode::OK);
        let provision_payload = response_json(provision_response).await;
        assert_eq!(provision_payload["tenantId"], tenant_id);
        assert_eq!(provision_payload["isolationLevel"], 4);
        assert_eq!(
            provision_payload["datasource"]["datasourceName"],
            datasource_name
        );
        assert_eq!(provision_payload["datasource"]["dbUri"], provision_db_url);
        assert_eq!(provision_payload["datasource"]["dbMaxConns"], 5);

        assert!(database_exists(&db_url, database_name.as_str()).await);

        let provision_db = Database::connect(provision_db_url.as_str())
            .await
            .expect("connect provisioned database");
        let current_database_row = provision_db
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Postgres,
                "SELECT current_database() AS db_name",
            ))
            .await
            .expect("query current database")
            .expect("row");
        let current_database: String = current_database_row
            .try_get("", "db_name")
            .expect("db_name");
        assert_eq!(current_database, database_name);

        let refresh_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tenant/runtime/refresh")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("refresh response");
        assert_eq!(refresh_response.status(), StatusCode::OK);
        let refresh_payload = response_json(refresh_response).await;
        assert!(
            refresh_payload["datasourceCount"]
                .as_u64()
                .expect("datasourceCount")
                >= 2,
            "unexpected refresh payload: {refresh_payload}"
        );

        let health_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/tenant/runtime/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("health response");
        assert_eq!(health_response.status(), StatusCode::OK);
        let health_payload = response_json(health_response).await;
        let datasource_health = health_payload
            .as_array()
            .expect("health array")
            .iter()
            .find(|item| item["datasource"] == datasource_name)
            .cloned()
            .expect("separate database datasource health");
        assert_eq!(
            datasource_health["reachable"], true,
            "unexpected health payload: {health_payload}"
        );

        let tenant = sys_tenant::Entity::find()
            .filter(sys_tenant::Column::TenantId.eq(tenant_id.as_str()))
            .one(&db)
            .await
            .expect("query tenant")
            .expect("tenant");
        assert_eq!(tenant.status, sys_tenant::TenantStatus::Enabled);
        assert_eq!(
            tenant.default_isolation_level,
            sys_tenant::TenantIsolationLevel::SeparateDatabase
        );

        let datasource = sys_tenant_datasource::Entity::find()
            .filter(sys_tenant_datasource::Column::TenantId.eq(tenant_id.as_str()))
            .one(&db)
            .await
            .expect("query datasource")
            .expect("datasource");
        assert_eq!(
            datasource.isolation_level,
            sys_tenant_datasource::TenantIsolationLevel::SeparateDatabase
        );
        assert_eq!(
            datasource.status,
            sys_tenant_datasource::TenantDatasourceStatus::Active
        );
        assert_eq!(
            datasource.datasource_name.as_deref(),
            Some(datasource_name.as_str())
        );
        assert_eq!(
            datasource.db_uri.as_deref(),
            Some(provision_db_url.as_str())
        );
        assert_eq!(datasource.db_max_conns, Some(5));

        drop(app);
        cleanup_tenant(&db_url, tenant_id.as_str(), None).await;
        cleanup_database(&db_url, database_name.as_str()).await;
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL and Redis"]
    async fn tenant_http_management_interfaces_roundtrip() {
        let db_url = e2e_database_url();
        let suffix = chrono::Utc::now().timestamp_millis();
        let tenant_id = format!("T-MGMT-{suffix}");
        let user_id = suffix;
        let datasource_name = format!("tenant_ds_{suffix}");
        cleanup_tenant(&db_url, tenant_id.as_str(), None).await;
        cleanup_user(&db_url, user_id).await;

        let db = Database::connect(&db_url).await.expect("connect database");
        sys_user::ActiveModel {
            id: Set(user_id),
            user_name: Set(format!("tenant_mgmt_user_{suffix}")),
            password: Set("argon2$test-password-hash".to_string()),
            nick_name: Set("Tenant Manager".to_string()),
            gender: Set(sys_user::Gender::Unknown),
            phone: Set(String::new()),
            email: Set(format!("tenant-mgmt-{suffix}@example.com")),
            avatar: Set(String::new()),
            status: Set(sys_user::UserStatus::Enabled),
            create_by: Set("test".to_string()),
            update_by: Set("test".to_string()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert sys user");

        let app = build_test_router(&db_url).await;

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tenant")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "tenantId": tenant_id,
                            "tenantName": "Mgmt Tenant",
                            "defaultIsolationLevel": 1,
                            "status": 1
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("create response");
        assert_eq!(create_response.status(), StatusCode::OK);

        let update_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/tenant/{tenant_id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "tenantName": "Mgmt Tenant Updated",
                            "contactName": "Ops",
                            "contactEmail": "ops@example.com",
                            "remark": "http-management-test"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("update response");
        assert_eq!(update_response.status(), StatusCode::OK);

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/tenant/{tenant_id}/status"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "status": 2
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("status response");
        assert_eq!(status_response.status(), StatusCode::OK);

        let datasource_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/tenant/{tenant_id}/datasource"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "isolationLevel": 2,
                            "datasourceName": datasource_name,
                            "remark": "datasource-bound"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("datasource response");
        assert_eq!(datasource_response.status(), StatusCode::OK);
        let datasource_payload = response_json(datasource_response).await;
        assert_eq!(
            datasource_payload["tenantId"], tenant_id,
            "unexpected datasource payload: {datasource_payload}"
        );
        assert_eq!(
            datasource_payload["isolationLevel"], 2,
            "unexpected datasource payload: {datasource_payload}"
        );
        assert_eq!(
            datasource_payload["datasourceName"], datasource_name,
            "unexpected datasource payload: {datasource_payload}"
        );
        assert_eq!(
            datasource_payload["status"], "inactive",
            "unexpected datasource payload: {datasource_payload}"
        );

        let membership_save_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/tenant/{tenant_id}/members"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "userId": user_id,
                            "roleCode": "owner",
                            "isDefault": true,
                            "status": 1,
                            "source": "manual"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("membership save response");
        assert_eq!(membership_save_response.status(), StatusCode::OK);

        let membership_list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/tenant/{tenant_id}/members"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("membership list response");
        assert_eq!(membership_list_response.status(), StatusCode::OK);
        let membership_payload = response_json(membership_list_response).await;
        assert_eq!(
            membership_payload[0]["tenantId"], tenant_id,
            "unexpected membership payload: {membership_payload}"
        );
        assert_eq!(
            membership_payload[0]["userId"], user_id,
            "unexpected membership payload: {membership_payload}"
        );
        assert_eq!(
            membership_payload[0]["roleCode"], "owner",
            "unexpected membership payload: {membership_payload}"
        );
        assert_eq!(
            membership_payload[0]["source"], "manual",
            "unexpected membership payload: {membership_payload}"
        );

        let detail_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/tenant/{tenant_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("detail response");
        assert_eq!(detail_response.status(), StatusCode::OK);
        let detail_payload = response_json(detail_response).await;
        assert_eq!(
            detail_payload["tenantName"], "Mgmt Tenant Updated",
            "unexpected detail payload: {detail_payload}"
        );
        assert_eq!(
            detail_payload["status"], 2,
            "unexpected detail payload: {detail_payload}"
        );
        assert_eq!(
            detail_payload["defaultIsolationLevel"], 2,
            "unexpected detail payload: {detail_payload}"
        );
        assert_eq!(
            detail_payload["datasource"]["datasourceName"], datasource_name,
            "unexpected detail payload: {detail_payload}"
        );
        assert_eq!(
            detail_payload["datasource"]["status"], "inactive",
            "unexpected detail payload: {detail_payload}"
        );

        let refresh_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/tenant/runtime/refresh")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("refresh response");
        assert_eq!(refresh_response.status(), StatusCode::OK);
        let refresh_payload = response_json(refresh_response).await;
        assert!(
            refresh_payload["tenantMetadataCount"]
                .as_u64()
                .expect("tenantMetadataCount")
                >= 1,
            "unexpected refresh payload: {refresh_payload}"
        );
        assert!(
            refresh_payload["datasourceCount"]
                .as_u64()
                .expect("datasourceCount")
                >= 1,
            "unexpected refresh payload: {refresh_payload}"
        );

        let health_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/tenant/runtime/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("health response");
        assert_eq!(health_response.status(), StatusCode::OK);
        let health_payload = response_json(health_response).await;
        assert!(
            health_payload
                .as_array()
                .expect("health array")
                .iter()
                .any(|item| item["datasource"] == "ds_test"),
            "unexpected health payload: {health_payload}"
        );

        let routes_response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/tenant/runtime/routes")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("routes response");
        assert_eq!(routes_response.status(), StatusCode::OK);
        let routes_payload = response_json(routes_response).await;
        assert!(
            routes_payload.is_array(),
            "unexpected routes payload: {routes_payload}"
        );

        cleanup_tenant(&db_url, tenant_id.as_str(), None).await;
        cleanup_user(&db_url, user_id).await;
    }
}
