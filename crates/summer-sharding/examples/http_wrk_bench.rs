use std::{collections::BTreeMap, env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use sea_orm::{
    ColumnTrait, ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend,
    EntityTrait, QueryFilter, QueryOrder, QueryResult, QuerySelect, Statement,
};
use serde::Deserialize;
use summer_sharding::{
    CurrentTenant, DataSourcePool, ShardingConfig, ShardingConnection, SummerShardingConfig,
    TenantContextLayer, TenantShardingConnection,
};
use summer_web::axum::{
    Extension, Router, extract::Query, http::HeaderMap, response::IntoResponse, routing::get,
};

const BENCH_SCHEMA: &str = "bench_perf";
const SEED_ROWS: i64 = 20_000;
const TENANT_A: &str = "T-BENCH-A";
const TENANT_B: &str = "T-BENCH-B";
const DEFAULT_DATABASE_URL: &str =
    "postgres://admin:123456@localhost/summerrs-admin?options=-c%20TimeZone%3DAsia%2FShanghai";
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:38080";
const DEFAULT_MAX_CONNECTIONS: u32 = 64;
const DEFAULT_MIN_CONNECTIONS: u32 = 8;
const DEFAULT_ACQUIRE_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 3_000;

mod bench_tenant_probe_entity {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(schema_name = "bench_perf", table_name = "tenant_probe")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub tenant_id: String,
        pub payload: String,
        pub updated_at: DateTime,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(Debug, Deserialize)]
struct IdQuery {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct RangeQuery {
    start: i64,
    end: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("SUMMER_SHARDING_BENCH_DATABASE_URL")
        .or_else(|_| env::var("SUMMER_SHARDING_E2E_DATABASE_URL"))
        .or_else(|_| env::var("DATABASE_URL"))
        .unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string());
    let bind_addr: SocketAddr = env::var("SUMMER_SHARDING_WRK_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
        .parse()?;

    let connect_options = build_connect_options(database_url.as_str());
    let raw = Database::connect(connect_options.clone()).await?;
    prepare_benchmark_schema(&raw).await;
    seed_benchmark_data(&raw).await;

    let runtime_config = Arc::new(runtime_config_from_str(
        tenant_config(database_url.as_str()).as_str(),
    ));
    let pool = DataSourcePool::from_connections(
        runtime_config.clone(),
        BTreeMap::from([("ds_bench".to_string(), raw.clone())]),
    )?;
    let sharding = ShardingConnection::with_pool(runtime_config, pool)?;
    sharding.reload_tenant_metadata(&raw).await?;

    let raw_router = Router::new()
        .route("/select", get(http_raw_select_handler))
        .route("/limit", get(http_raw_limit_handler))
        .route("/entity/select", get(http_raw_entity_select_handler))
        .route("/entity/limit", get(http_raw_entity_limit_handler))
        .layer(Extension(raw.clone()));
    let tenant_router = Router::new()
        .route("/select", get(http_tenant_select_handler))
        .route("/limit", get(http_tenant_limit_handler))
        .route("/entity/select", get(http_tenant_entity_select_handler))
        .route("/entity/limit", get(http_tenant_entity_limit_handler))
        .layer(TenantContextLayer::from_header().with_sharding(sharding));
    let app = Router::new()
        .route("/health", get(health_handler))
        .nest("/raw", raw_router)
        .nest("/tenant", tenant_router);

    println!("summer-sharding wrk bench server listening on http://{bind_addr}");
    println!("raw select:    http://{bind_addr}/raw/select?id=1024");
    println!("tenant select: http://{bind_addr}/tenant/select?id=1024");
    println!("raw entity:    http://{bind_addr}/raw/entity/select?id=1024");
    println!("tenant entity: http://{bind_addr}/tenant/entity/select?id=1024");
    println!("raw limit:     http://{bind_addr}/raw/limit?start=1&end=20");
    println!("tenant limit:  http://{bind_addr}/tenant/limit?start=1&end=20");
    println!("raw e-limit:   http://{bind_addr}/raw/entity/limit?start=1&end=20");
    println!("tenant e-limit:http://{bind_addr}/tenant/entity/limit?start=1&end=20");
    println!("use header:    x-tenant-id: {TENANT_A}");
    println!(
        "pool tuning:   max_connections={} min_connections={} acquire_timeout_ms={} connect_timeout_ms={} test_before_acquire={}",
        connect_options
            .get_max_connections()
            .unwrap_or(DEFAULT_MAX_CONNECTIONS),
        connect_options
            .get_min_connections()
            .unwrap_or(DEFAULT_MIN_CONNECTIONS),
        connect_options
            .get_acquire_timeout()
            .unwrap_or(Duration::from_millis(DEFAULT_ACQUIRE_TIMEOUT_MS))
            .as_millis(),
        connect_options
            .get_connect_timeout()
            .unwrap_or(Duration::from_millis(DEFAULT_CONNECT_TIMEOUT_MS))
            .as_millis(),
        read_bool_env("SUMMER_SHARDING_BENCH_TEST_BEFORE_ACQUIRE", false),
    );

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    summer_web::axum::serve(listener, app).await?;
    Ok(())
}

async fn health_handler() -> impl IntoResponse {
    "ok"
}

async fn http_raw_select_handler(
    Extension(raw): Extension<DatabaseConnection>,
    headers: HeaderMap,
    Query(IdQuery { id }): Query<IdQuery>,
) -> impl IntoResponse {
    let row = raw
        .query_one_raw(tenant_probe_select_by_id_raw_stmt(
            tenant_id_from_headers(&headers),
            id,
        ))
        .await
        .expect("raw http tenant select")
        .expect("raw http tenant row");
    payload_from_row(row)
}

async fn http_tenant_select_handler(
    CurrentTenant(_tenant): CurrentTenant,
    TenantShardingConnection(sharding): TenantShardingConnection,
    Query(IdQuery { id }): Query<IdQuery>,
) -> impl IntoResponse {
    let row = sharding
        .query_one_raw(tenant_probe_select_by_id_sharding_stmt(id))
        .await
        .expect("tenant http select")
        .expect("tenant http row");
    payload_from_row(row)
}

async fn http_raw_limit_handler(
    Extension(raw): Extension<DatabaseConnection>,
    headers: HeaderMap,
    Query(RangeQuery { start, end }): Query<RangeQuery>,
) -> impl IntoResponse {
    let rows = raw
        .query_all_raw(tenant_probe_limit_raw_stmt(
            tenant_id_from_headers(&headers),
            start,
            end,
        ))
        .await
        .expect("raw http tenant limit");
    rows.len().to_string()
}

async fn http_tenant_limit_handler(
    CurrentTenant(_tenant): CurrentTenant,
    TenantShardingConnection(sharding): TenantShardingConnection,
    Query(RangeQuery { start, end }): Query<RangeQuery>,
) -> impl IntoResponse {
    let rows = sharding
        .query_all_raw(tenant_probe_limit_sharding_stmt(start, end))
        .await
        .expect("tenant http limit");
    rows.len().to_string()
}

async fn http_raw_entity_select_handler(
    Extension(raw): Extension<DatabaseConnection>,
    headers: HeaderMap,
    Query(IdQuery { id }): Query<IdQuery>,
) -> impl IntoResponse {
    let row = bench_tenant_probe_entity::Entity::find()
        .filter(bench_tenant_probe_entity::Column::TenantId.eq(tenant_id_from_headers(&headers)))
        .filter(bench_tenant_probe_entity::Column::Id.eq(id))
        .one(&raw)
        .await
        .expect("raw entity select")
        .expect("raw entity row");
    row.payload
}

async fn http_tenant_entity_select_handler(
    CurrentTenant(_tenant): CurrentTenant,
    TenantShardingConnection(sharding): TenantShardingConnection,
    Query(IdQuery { id }): Query<IdQuery>,
) -> impl IntoResponse {
    let row = bench_tenant_probe_entity::Entity::find_by_id(id)
        .one(&sharding)
        .await
        .expect("tenant entity select")
        .expect("tenant entity row");
    row.payload
}

async fn http_raw_entity_limit_handler(
    Extension(raw): Extension<DatabaseConnection>,
    headers: HeaderMap,
    Query(RangeQuery { start, end }): Query<RangeQuery>,
) -> impl IntoResponse {
    let rows = bench_tenant_probe_entity::Entity::find()
        .filter(bench_tenant_probe_entity::Column::TenantId.eq(tenant_id_from_headers(&headers)))
        .filter(bench_tenant_probe_entity::Column::Id.between(start, end))
        .order_by_asc(bench_tenant_probe_entity::Column::Id)
        .limit(20)
        .all(&raw)
        .await
        .expect("raw entity limit");
    rows.len().to_string()
}

async fn http_tenant_entity_limit_handler(
    CurrentTenant(_tenant): CurrentTenant,
    TenantShardingConnection(sharding): TenantShardingConnection,
    Query(RangeQuery { start, end }): Query<RangeQuery>,
) -> impl IntoResponse {
    let rows = bench_tenant_probe_entity::Entity::find()
        .filter(bench_tenant_probe_entity::Column::Id.between(start, end))
        .order_by_asc(bench_tenant_probe_entity::Column::Id)
        .limit(20)
        .all(&sharding)
        .await
        .expect("tenant entity limit");
    rows.len().to_string()
}

fn tenant_id_from_headers(headers: &HeaderMap) -> &str {
    headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .expect("x-tenant-id header")
}

fn payload_from_row(row: QueryResult) -> String {
    row.try_get("", "payload").expect("payload")
}

fn build_connect_options(database_url: &str) -> ConnectOptions {
    let mut options = ConnectOptions::new(database_url.to_string());
    options.max_connections(read_u32_env(
        "SUMMER_SHARDING_BENCH_MAX_CONNECTIONS",
        DEFAULT_MAX_CONNECTIONS,
    ));
    options.min_connections(read_u32_env(
        "SUMMER_SHARDING_BENCH_MIN_CONNECTIONS",
        DEFAULT_MIN_CONNECTIONS,
    ));
    options.acquire_timeout(Duration::from_millis(read_u64_env(
        "SUMMER_SHARDING_BENCH_ACQUIRE_TIMEOUT_MS",
        DEFAULT_ACQUIRE_TIMEOUT_MS,
    )));
    options.connect_timeout(Duration::from_millis(read_u64_env(
        "SUMMER_SHARDING_BENCH_CONNECT_TIMEOUT_MS",
        DEFAULT_CONNECT_TIMEOUT_MS,
    )));
    options.test_before_acquire(read_bool_env(
        "SUMMER_SHARDING_BENCH_TEST_BEFORE_ACQUIRE",
        false,
    ));
    options.sqlx_logging(false);
    options
}

fn read_u32_env(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default)
}

fn read_u64_env(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn read_bool_env(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

async fn prepare_benchmark_schema(connection: &DatabaseConnection) {
    connection
        .execute_unprepared(
            format!(
                r#"
                CREATE SCHEMA IF NOT EXISTS {schema};

                DROP TABLE IF EXISTS {schema}.tenant_probe;

                CREATE TABLE {schema}.tenant_probe (
                    id BIGINT PRIMARY KEY,
                    tenant_id VARCHAR(64) NOT NULL,
                    payload VARCHAR(255) NOT NULL,
                    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE INDEX tenant_probe_tenant_id_id_idx
                    ON {schema}.tenant_probe(tenant_id, id);
                "#,
                schema = BENCH_SCHEMA,
            )
            .as_str(),
        )
        .await
        .expect("prepare benchmark schema");
}

async fn seed_benchmark_data(connection: &DatabaseConnection) {
    connection
        .execute_unprepared(
            format!(
                r#"
                INSERT INTO {schema}.tenant_probe(id, tenant_id, payload, updated_at)
                SELECT gs, '{tenant_a}', 'tenant-a-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs;

                INSERT INTO {schema}.tenant_probe(id, tenant_id, payload, updated_at)
                SELECT gs + {seed_rows}, '{tenant_b}', 'tenant-b-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs;
                "#,
                schema = BENCH_SCHEMA,
                seed_rows = SEED_ROWS,
                tenant_a = TENANT_A,
                tenant_b = TENANT_B,
            )
            .as_str(),
        )
        .await
        .expect("seed benchmark data");
}

fn tenant_probe_select_by_id_raw_stmt(tenant_id: &str, id: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT id, payload FROM {BENCH_SCHEMA}.tenant_probe WHERE tenant_id = $1 AND id = $2"
        ),
        [tenant_id.into(), id.into()],
    )
}

fn tenant_probe_select_by_id_sharding_stmt(id: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!("SELECT id, payload FROM {BENCH_SCHEMA}.tenant_probe WHERE id = $1"),
        [id.into()],
    )
}

fn tenant_probe_limit_raw_stmt(tenant_id: &str, start: i64, end: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT id, payload FROM {BENCH_SCHEMA}.tenant_probe WHERE tenant_id = $1 AND id BETWEEN $2 AND $3 ORDER BY id LIMIT 20"
        ),
        [tenant_id.into(), start.into(), end.into()],
    )
}

fn tenant_probe_limit_sharding_stmt(start: i64, end: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT id, payload FROM {BENCH_SCHEMA}.tenant_probe WHERE id BETWEEN $1 AND $2 ORDER BY id LIMIT 20"
        ),
        [start.into(), end.into()],
    )
}

fn tenant_config(database_url: &str) -> String {
    format!(
        r#"
        [datasources.ds_bench]
        uri = "{database_url}"
        schema = "{schema}"
        role = "primary"

        [tenant]
        enabled = true
        default_isolation = "shared_row"

        [tenant.row_level]
        column_name = "tenant_id"
        strategy = "sql_rewrite"

        [sharding.global]
        default_datasource = "ds_bench"
        "#,
        schema = BENCH_SCHEMA,
    )
}

fn runtime_config_from_str(input: &str) -> ShardingConfig {
    let wrapped = wrap_bench_config(input);
    let path = write_temp_config(wrapped.as_str());
    let registry = summer::config::toml::TomlConfigRegistry::new(
        path.as_path(),
        summer::config::env::Env::from_string("dev"),
    )
    .expect("build benchmark config registry");
    let config = summer::config::ConfigRegistry::get_config::<SummerShardingConfig>(&registry)
        .expect("load benchmark sharding config");
    let runtime = config
        .into_runtime_config()
        .expect("convert benchmark runtime config");
    let _ = std::fs::remove_file(&path);
    runtime
}

fn wrap_bench_config(input: &str) -> String {
    let mut output = String::from("[summer-sharding]\n");
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
            output.push_str(&line.replacen("[[", "[[summer-sharding.", 1));
            output.push('\n');
        } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
            output.push_str(&line.replacen('[', "[summer-sharding.", 1));
            output.push('\n');
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

fn write_temp_config(content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "summer-sharding-wrk-bench-{}-{}.toml",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::write(&path, content).expect("write benchmark temp config");
    path
}
