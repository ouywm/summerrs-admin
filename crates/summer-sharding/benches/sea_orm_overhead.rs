use std::{
    hint::black_box,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicU64, Ordering},
    },
    time::Duration,
};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
#[cfg(feature = "web")]
use sea_orm::QueryResult;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};
#[cfg(feature = "web")]
use serde::Deserialize;
#[cfg(feature = "web")]
use summer_sharding::{CurrentTenant, TenantContextLayer, TenantShardingConnection};
use summer_sharding::{
    ShardingConfig, ShardingConnection, SummerShardingConfig, TenantContext, TenantIsolationLevel,
};
#[cfg(feature = "web")]
use summer_web::axum::{
    Extension, Router,
    body::{Body, to_bytes},
    extract::Query,
    http::{HeaderMap, Request},
    response::IntoResponse,
    routing::get,
};
#[cfg(feature = "web")]
use tower::util::ServiceExt;

const BENCH_SCHEMA: &str = "bench_perf";
const TENANT_A: &str = "T-BENCH-A";
const DEFAULT_DATABASE_URL: &str =
    "postgres://admin:123456@localhost/summerrs-admin?options=-c%20TimeZone%3DAsia%2FShanghai";
const SEED_ROWS: i64 = 10_000;
const LIMIT_WINDOW: i64 = 79;
const GROUP_SAMPLE_SIZE: usize = 10;
const GROUP_WARMUP_MS: u64 = 500;
const GROUP_MEASURE_SECS: u64 = 2;

#[derive(Clone)]
struct BenchmarkEnvironment {
    raw: DatabaseConnection,
    passthrough: ShardingConnection,
    tenant: ShardingConnection,
    sharded: ShardingConnection,
    passthrough_read_cursor: Arc<AtomicI64>,
    passthrough_limit_cursor: Arc<AtomicI64>,
    passthrough_raw_insert: Arc<AtomicI64>,
    passthrough_sharding_insert: Arc<AtomicI64>,
    tenant_read_cursor: Arc<AtomicI64>,
    tenant_limit_cursor: Arc<AtomicI64>,
    tenant_raw_insert: Arc<AtomicI64>,
    tenant_sharding_insert: Arc<AtomicI64>,
    route_read_cursor: Arc<AtomicI64>,
    route_limit_cursor: Arc<AtomicI64>,
    route_raw_insert: Arc<AtomicI64>,
    route_sharding_insert: Arc<AtomicI64>,
}

impl BenchmarkEnvironment {
    async fn build() -> Self {
        let database_url = benchmark_database_url();
        let raw = Database::connect(&database_url)
            .await
            .expect("connect benchmark database");

        prepare_benchmark_schema(&raw).await;
        seed_benchmark_data(&raw).await;

        let passthrough = ShardingConnection::build(
            runtime_config_from_str(passthrough_config(database_url.as_str()).as_str()),
            raw.clone(),
        )
        .await
        .expect("build passthrough sharding connection");
        let tenant = ShardingConnection::build(
            runtime_config_from_str(tenant_config(database_url.as_str()).as_str()),
            raw.clone(),
        )
        .await
        .expect("build tenant sharding connection");
        let sharded = ShardingConnection::build(
            runtime_config_from_str(sharded_config(database_url.as_str()).as_str()),
            raw.clone(),
        )
        .await
        .expect("build sharded connection");

        warmup_connections(&raw, &passthrough, &tenant, &sharded).await;

        Self {
            raw,
            passthrough,
            tenant,
            sharded,
            passthrough_read_cursor: Arc::new(AtomicI64::new(1)),
            passthrough_limit_cursor: Arc::new(AtomicI64::new(1)),
            passthrough_raw_insert: Arc::new(AtomicI64::new(1_000_000)),
            passthrough_sharding_insert: Arc::new(AtomicI64::new(2_000_000)),
            tenant_read_cursor: Arc::new(AtomicI64::new(1)),
            tenant_limit_cursor: Arc::new(AtomicI64::new(1)),
            tenant_raw_insert: Arc::new(AtomicI64::new(3_000_000)),
            tenant_sharding_insert: Arc::new(AtomicI64::new(4_000_000)),
            route_read_cursor: Arc::new(AtomicI64::new(1)),
            route_limit_cursor: Arc::new(AtomicI64::new(1)),
            route_raw_insert: Arc::new(AtomicI64::new(5_000_000)),
            route_sharding_insert: Arc::new(AtomicI64::new(6_000_000)),
        }
    }
}

fn benchmark_database_url() -> String {
    std::env::var("SUMMER_SHARDING_BENCH_DATABASE_URL")
        .or_else(|_| std::env::var("SUMMER_SHARDING_E2E_DATABASE_URL"))
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string())
}

fn benchmark_sharding_overhead(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let env = Arc::new(rt.block_on(BenchmarkEnvironment::build()));

    bench_passthrough(c, &rt, env.clone());
    bench_tenant_rewrite(c, &rt, env.clone());
    #[cfg(feature = "web")]
    bench_http(c, &rt, env.clone());
    bench_hash_route(c, &rt, env);
}

fn bench_passthrough(
    c: &mut Criterion,
    rt: &tokio::runtime::Runtime,
    env: Arc<BenchmarkEnvironment>,
) {
    let mut group = c.benchmark_group("passthrough/select_by_id");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.passthrough_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let row = env
                .raw
                .query_one_raw(raw_probe_select_by_id_stmt(id))
                .await
                .expect("raw select by id");
            black_box(row.expect("row"));
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.passthrough_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let row = env
                .passthrough
                .query_one_raw(raw_probe_select_by_id_stmt(id))
                .await
                .expect("sharding select by id");
            black_box(row.expect("row"));
        });
    });
    group.finish();

    let mut group = c.benchmark_group("passthrough/select_limit_20");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.passthrough_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                20,
            ));
            let rows = env
                .raw
                .query_all_raw(raw_probe_limit_stmt(start, end))
                .await
                .expect("raw limit query");
            black_box(rows.len());
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.passthrough_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                20,
            ));
            let rows = env
                .passthrough
                .query_all_raw(raw_probe_limit_stmt(start, end))
                .await
                .expect("sharding limit query");
            black_box(rows.len());
        });
    });
    group.finish();

    let mut group = c.benchmark_group("passthrough/insert_one");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_insert_id(env.passthrough_raw_insert.as_ref()));
            env.raw
                .execute_raw(raw_probe_insert_stmt(id, format!("raw-insert-{id}")))
                .await
                .expect("raw insert");
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_insert_id(env.passthrough_sharding_insert.as_ref()));
            env.passthrough
                .execute_raw(raw_probe_insert_stmt(id, format!("sharding-insert-{id}")))
                .await
                .expect("sharding insert");
        });
    });
    group.finish();

    let mut group = c.benchmark_group("passthrough/update_by_id");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.passthrough_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            env.raw
                .execute_raw(raw_probe_update_stmt(id, format!("raw-update-{id}")))
                .await
                .expect("raw update");
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.passthrough_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            env.passthrough
                .execute_raw(raw_probe_update_stmt(id, format!("sharding-update-{id}")))
                .await
                .expect("sharding update");
        });
    });
    group.finish();
}

fn bench_tenant_rewrite(
    c: &mut Criterion,
    rt: &tokio::runtime::Runtime,
    env: Arc<BenchmarkEnvironment>,
) {
    let mut group = c.benchmark_group("tenant_rewrite/select_by_id");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.tenant_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let row = env
                .raw
                .query_one_raw(tenant_probe_select_by_id_raw_stmt(TENANT_A, id))
                .await
                .expect("raw tenant select");
            black_box(row.expect("row"));
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.tenant_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let row = env
                .tenant
                .with_tenant_context(TenantContext::new(
                    TENANT_A,
                    TenantIsolationLevel::SharedRow,
                ))
                .query_one_raw(tenant_probe_select_by_id_sharding_stmt(id))
                .await
                .expect("sharding tenant select");
            black_box(row.expect("row"));
        });
    });
    group.finish();

    let mut group = c.benchmark_group("tenant_rewrite/select_limit_20");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.tenant_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                20,
            ));
            let rows = env
                .raw
                .query_all_raw(tenant_probe_limit_raw_stmt(TENANT_A, start, end))
                .await
                .expect("raw tenant limit");
            black_box(rows.len());
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.tenant_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                20,
            ));
            let rows = env
                .tenant
                .with_tenant_context(TenantContext::new(
                    TENANT_A,
                    TenantIsolationLevel::SharedRow,
                ))
                .query_all_raw(tenant_probe_limit_sharding_stmt(start, end))
                .await
                .expect("sharding tenant limit");
            black_box(rows.len());
        });
    });
    group.finish();

    let mut group = c.benchmark_group("tenant_rewrite/insert_one");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_insert_id(env.tenant_raw_insert.as_ref()));
            env.raw
                .execute_raw(tenant_probe_insert_raw_stmt(
                    TENANT_A,
                    id,
                    format!("tenant-raw-insert-{id}"),
                ))
                .await
                .expect("raw tenant insert");
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_insert_id(env.tenant_sharding_insert.as_ref()));
            env.tenant
                .with_tenant_context(TenantContext::new(
                    TENANT_A,
                    TenantIsolationLevel::SharedRow,
                ))
                .execute_raw(tenant_probe_insert_sharding_stmt(
                    id,
                    format!("tenant-sharding-insert-{id}"),
                ))
                .await
                .expect("sharding tenant insert");
        });
    });
    group.finish();

    let mut group = c.benchmark_group("tenant_rewrite/update_by_id");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.tenant_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            env.raw
                .execute_raw(tenant_probe_update_raw_stmt(
                    TENANT_A,
                    id,
                    format!("tenant-raw-update-{id}"),
                ))
                .await
                .expect("raw tenant update");
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.tenant_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            env.tenant
                .with_tenant_context(TenantContext::new(
                    TENANT_A,
                    TenantIsolationLevel::SharedRow,
                ))
                .execute_raw(tenant_probe_update_sharding_stmt(
                    id,
                    format!("tenant-sharding-update-{id}"),
                ))
                .await
                .expect("sharding tenant update");
        });
    });
    group.finish();
}

fn bench_hash_route(
    c: &mut Criterion,
    rt: &tokio::runtime::Runtime,
    env: Arc<BenchmarkEnvironment>,
) {
    let mut group = c.benchmark_group("hash_route/select_by_id");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.route_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let row = env
                .raw
                .query_one_raw(route_probe_select_by_id_raw_stmt(id))
                .await
                .expect("raw route select");
            black_box(row.expect("row"));
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.route_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let row = env
                .sharded
                .query_one_raw(route_probe_select_by_id_sharding_stmt(id))
                .await
                .expect("sharding route select");
            black_box(row.expect("row"));
        });
    });
    group.finish();

    let mut group = c.benchmark_group("hash_route/select_limit_20");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.route_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                LIMIT_WINDOW,
            ));
            let rows = env
                .raw
                .query_all_raw(route_probe_limit_raw_stmt(start, end))
                .await
                .expect("raw route limit");
            black_box(rows.len());
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.route_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                LIMIT_WINDOW,
            ));
            let rows = env
                .sharded
                .query_all_raw(route_probe_limit_sharding_stmt(start, end))
                .await
                .expect("sharding route limit");
            black_box(rows.len());
        });
    });
    group.finish();

    let mut group = c.benchmark_group("hash_route/insert_one");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_insert_id(env.route_raw_insert.as_ref()));
            env.raw
                .execute_raw(route_probe_insert_raw_stmt(
                    id,
                    format!("route-raw-insert-{id}"),
                ))
                .await
                .expect("raw route insert");
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_insert_id(env.route_sharding_insert.as_ref()));
            env.sharded
                .execute_raw(route_probe_insert_sharding_stmt(
                    id,
                    format!("route-sharding-insert-{id}"),
                ))
                .await
                .expect("sharding route insert");
        });
    });
    group.finish();

    let mut group = c.benchmark_group("hash_route/update_by_id");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.route_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            env.raw
                .execute_raw(route_probe_update_raw_stmt(
                    id,
                    format!("route-raw-update-{id}"),
                ))
                .await
                .expect("raw route update");
        });
    });
    group.bench_function("sharding", |b| {
        let env = env.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.route_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            env.sharded
                .execute_raw(route_probe_update_sharding_stmt(
                    id,
                    format!("route-sharding-update-{id}"),
                ))
                .await
                .expect("sharding route update");
        });
    });
    group.finish();
}

#[cfg(feature = "web")]
fn bench_http(c: &mut Criterion, rt: &tokio::runtime::Runtime, env: Arc<BenchmarkEnvironment>) {
    let raw_context_router = build_http_raw_context_router();
    let tenant_context_router = build_http_tenant_context_router(None);
    let tenant_context_with_sharding_router =
        build_http_tenant_context_router(Some(env.tenant.clone()));
    let raw_select_router = build_http_raw_select_router(env.raw.clone());
    let manual_bind_select_router = build_http_manual_bind_select_router(env.tenant.clone());
    let auto_inject_select_router = build_http_auto_inject_select_router(env.tenant.clone());
    let raw_limit_router = build_http_raw_limit_router(env.raw.clone());
    let manual_bind_limit_router = build_http_manual_bind_limit_router(env.tenant.clone());
    let auto_inject_limit_router = build_http_auto_inject_limit_router(env.tenant.clone());

    let mut group = c.benchmark_group("http/tenant_context");
    configure_group(&mut group);
    group.bench_function("raw_header", |b| {
        let router = raw_context_router.clone();
        b.to_async(rt).iter(|| async {
            let response = router
                .clone()
                .oneshot(http_context_request())
                .await
                .expect("raw context response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("raw context body");
            black_box(body);
        });
    });
    group.bench_function("tenant_layer", |b| {
        let router = tenant_context_router.clone();
        b.to_async(rt).iter(|| async {
            let response = router
                .clone()
                .oneshot(http_context_request())
                .await
                .expect("tenant context response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("tenant context body");
            black_box(body);
        });
    });
    group.bench_function("tenant_layer_with_sharding", |b| {
        let router = tenant_context_with_sharding_router.clone();
        b.to_async(rt).iter(|| async {
            let response = router
                .clone()
                .oneshot(http_context_request())
                .await
                .expect("tenant context + sharding response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("tenant context + sharding body");
            black_box(body);
        });
    });
    group.finish();

    let mut group = c.benchmark_group("http/tenant_select_by_id");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        let router = raw_select_router.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.tenant_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let response = router
                .clone()
                .oneshot(http_select_request(id))
                .await
                .expect("raw select response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("raw select body");
            black_box(body);
        });
    });
    group.bench_function("manual_bind", |b| {
        let env = env.clone();
        let router = manual_bind_select_router.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.tenant_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let response = router
                .clone()
                .oneshot(http_select_request(id))
                .await
                .expect("manual bind select response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("manual bind select body");
            black_box(body);
        });
    });
    group.bench_function("auto_inject", |b| {
        let env = env.clone();
        let router = auto_inject_select_router.clone();
        b.to_async(rt).iter(|| async {
            let id = black_box(next_existing_id(
                env.tenant_read_cursor.as_ref(),
                1,
                SEED_ROWS,
            ));
            let response = router
                .clone()
                .oneshot(http_select_request(id))
                .await
                .expect("auto inject select response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("auto inject select body");
            black_box(body);
        });
    });
    group.finish();

    let mut group = c.benchmark_group("http/tenant_select_limit_20");
    configure_group(&mut group);
    group.bench_function("raw", |b| {
        let env = env.clone();
        let router = raw_limit_router.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.tenant_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                20,
            ));
            let response = router
                .clone()
                .oneshot(http_limit_request(start, end))
                .await
                .expect("raw limit response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("raw limit body");
            black_box(body);
        });
    });
    group.bench_function("manual_bind", |b| {
        let env = env.clone();
        let router = manual_bind_limit_router.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.tenant_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                20,
            ));
            let response = router
                .clone()
                .oneshot(http_limit_request(start, end))
                .await
                .expect("manual bind limit response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("manual bind limit body");
            black_box(body);
        });
    });
    group.bench_function("auto_inject", |b| {
        let env = env.clone();
        let router = auto_inject_limit_router.clone();
        b.to_async(rt).iter(|| async {
            let (start, end) = black_box(next_limit_window(
                env.tenant_limit_cursor.as_ref(),
                1,
                SEED_ROWS,
                20,
            ));
            let response = router
                .clone()
                .oneshot(http_limit_request(start, end))
                .await
                .expect("auto inject limit response");
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("auto inject limit body");
            black_box(body);
        });
    });
    group.finish();
}

fn configure_group(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group.sample_size(GROUP_SAMPLE_SIZE);
    group.warm_up_time(Duration::from_millis(GROUP_WARMUP_MS));
    group.measurement_time(Duration::from_secs(GROUP_MEASURE_SECS));
    group.throughput(Throughput::Elements(1));
}

async fn prepare_benchmark_schema(connection: &DatabaseConnection) {
    connection
        .execute_unprepared(
            format!(
                r#"
                CREATE SCHEMA IF NOT EXISTS {schema};

                DROP TABLE IF EXISTS {schema}.raw_probe;
                DROP TABLE IF EXISTS {schema}.tenant_probe;
                DROP TABLE IF EXISTS {schema}.route_probe_0;
                DROP TABLE IF EXISTS {schema}.route_probe_1;
                DROP TABLE IF EXISTS {schema}.route_probe_2;
                DROP TABLE IF EXISTS {schema}.route_probe_3;

                CREATE TABLE {schema}.raw_probe (
                    id BIGINT PRIMARY KEY,
                    payload VARCHAR(255) NOT NULL,
                    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE {schema}.tenant_probe (
                    id BIGINT PRIMARY KEY,
                    tenant_id VARCHAR(64) NOT NULL,
                    payload VARCHAR(255) NOT NULL,
                    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE INDEX tenant_probe_tenant_id_id_idx
                    ON {schema}.tenant_probe(tenant_id, id);

                CREATE TABLE {schema}.route_probe_0 (
                    id BIGINT PRIMARY KEY,
                    payload VARCHAR(255) NOT NULL,
                    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE {schema}.route_probe_1 (
                    id BIGINT PRIMARY KEY,
                    payload VARCHAR(255) NOT NULL,
                    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE {schema}.route_probe_2 (
                    id BIGINT PRIMARY KEY,
                    payload VARCHAR(255) NOT NULL,
                    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE {schema}.route_probe_3 (
                    id BIGINT PRIMARY KEY,
                    payload VARCHAR(255) NOT NULL,
                    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
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
                INSERT INTO {schema}.raw_probe(id, payload, updated_at)
                SELECT gs, 'raw-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs;

                INSERT INTO {schema}.tenant_probe(id, tenant_id, payload, updated_at)
                SELECT gs, '{tenant_a}', 'tenant-a-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs;

                INSERT INTO {schema}.tenant_probe(id, tenant_id, payload, updated_at)
                SELECT gs + {seed_rows}, '{tenant_b}', 'tenant-b-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs;

                INSERT INTO {schema}.route_probe_0(id, payload, updated_at)
                SELECT gs, 'route-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs
                WHERE mod(gs, 4) = 0;

                INSERT INTO {schema}.route_probe_1(id, payload, updated_at)
                SELECT gs, 'route-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs
                WHERE mod(gs, 4) = 1;

                INSERT INTO {schema}.route_probe_2(id, payload, updated_at)
                SELECT gs, 'route-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs
                WHERE mod(gs, 4) = 2;

                INSERT INTO {schema}.route_probe_3(id, payload, updated_at)
                SELECT gs, 'route-' || gs::text, CURRENT_TIMESTAMP
                FROM generate_series(1, {seed_rows}) AS gs
                WHERE mod(gs, 4) = 3;
                "#,
                schema = BENCH_SCHEMA,
                seed_rows = SEED_ROWS,
                tenant_a = TENANT_A,
                tenant_b = "T-BENCH-B",
            )
            .as_str(),
        )
        .await
        .expect("seed benchmark data");
}

async fn warmup_connections(
    raw: &DatabaseConnection,
    passthrough: &ShardingConnection,
    tenant: &ShardingConnection,
    sharded: &ShardingConnection,
) {
    raw.query_one_raw(raw_probe_select_by_id_stmt(1))
        .await
        .expect("warm raw");
    passthrough
        .query_one_raw(raw_probe_select_by_id_stmt(1))
        .await
        .expect("warm passthrough");
    tenant
        .with_tenant_context(TenantContext::new(
            TENANT_A,
            TenantIsolationLevel::SharedRow,
        ))
        .query_one_raw(tenant_probe_select_by_id_sharding_stmt(1))
        .await
        .expect("warm tenant");
    sharded
        .query_one_raw(route_probe_select_by_id_sharding_stmt(1))
        .await
        .expect("warm sharded");
}

fn raw_probe_select_by_id_stmt(id: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!("SELECT id, payload FROM {BENCH_SCHEMA}.raw_probe WHERE id = $1"),
        [id.into()],
    )
}

fn raw_probe_limit_stmt(start: i64, end: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT id, payload FROM {BENCH_SCHEMA}.raw_probe WHERE id BETWEEN $1 AND $2 ORDER BY id LIMIT 20"
        ),
        [start.into(), end.into()],
    )
}

fn raw_probe_insert_stmt(id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "INSERT INTO {BENCH_SCHEMA}.raw_probe(id, payload, updated_at) VALUES ($1, $2, CURRENT_TIMESTAMP)"
        ),
        [id.into(), payload.into()],
    )
}

fn raw_probe_update_stmt(id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "UPDATE {BENCH_SCHEMA}.raw_probe SET payload = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2"
        ),
        [payload.into(), id.into()],
    )
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

fn tenant_probe_insert_raw_stmt(tenant_id: &str, id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "INSERT INTO {BENCH_SCHEMA}.tenant_probe(id, tenant_id, payload, updated_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP)"
        ),
        [id.into(), tenant_id.into(), payload.into()],
    )
}

fn tenant_probe_insert_sharding_stmt(id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "INSERT INTO {BENCH_SCHEMA}.tenant_probe(id, payload, updated_at) VALUES ($1, $2, CURRENT_TIMESTAMP)"
        ),
        [id.into(), payload.into()],
    )
}

fn tenant_probe_update_raw_stmt(tenant_id: &str, id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "UPDATE {BENCH_SCHEMA}.tenant_probe SET payload = $1, updated_at = CURRENT_TIMESTAMP WHERE tenant_id = $2 AND id = $3"
        ),
        [payload.into(), tenant_id.into(), id.into()],
    )
}

fn tenant_probe_update_sharding_stmt(id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "UPDATE {BENCH_SCHEMA}.tenant_probe SET payload = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2"
        ),
        [payload.into(), id.into()],
    )
}

fn route_probe_select_by_id_raw_stmt(id: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT id, payload FROM {BENCH_SCHEMA}.route_probe_{} WHERE id = $1",
            route_shard_index(id)
        ),
        [id.into()],
    )
}

fn route_probe_select_by_id_sharding_stmt(id: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!("SELECT id, payload FROM {BENCH_SCHEMA}.route_probe WHERE id = $1"),
        [id.into()],
    )
}

fn route_probe_limit_raw_stmt(start: i64, end: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            r#"
            SELECT id, payload
            FROM (
                SELECT id, payload FROM {schema}.route_probe_0 WHERE id BETWEEN $1 AND $2
                UNION ALL
                SELECT id, payload FROM {schema}.route_probe_1 WHERE id BETWEEN $3 AND $4
                UNION ALL
                SELECT id, payload FROM {schema}.route_probe_2 WHERE id BETWEEN $5 AND $6
                UNION ALL
                SELECT id, payload FROM {schema}.route_probe_3 WHERE id BETWEEN $7 AND $8
            ) AS merged
            ORDER BY id
            LIMIT 20
            "#,
            schema = BENCH_SCHEMA,
        ),
        [
            start.into(),
            end.into(),
            start.into(),
            end.into(),
            start.into(),
            end.into(),
            start.into(),
            end.into(),
        ],
    )
}

fn route_probe_limit_sharding_stmt(start: i64, end: i64) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT id, payload FROM {BENCH_SCHEMA}.route_probe WHERE id BETWEEN $1 AND $2 ORDER BY id LIMIT 20"
        ),
        [start.into(), end.into()],
    )
}

fn route_probe_insert_raw_stmt(id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "INSERT INTO {BENCH_SCHEMA}.route_probe_{}(id, payload, updated_at) VALUES ($1, $2, CURRENT_TIMESTAMP)",
            route_shard_index(id)
        ),
        [id.into(), payload.into()],
    )
}

fn route_probe_insert_sharding_stmt(id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "INSERT INTO {BENCH_SCHEMA}.route_probe(id, payload, updated_at) VALUES ($1, $2, CURRENT_TIMESTAMP)"
        ),
        [id.into(), payload.into()],
    )
}

fn route_probe_update_raw_stmt(id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "UPDATE {BENCH_SCHEMA}.route_probe_{} SET payload = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
            route_shard_index(id)
        ),
        [payload.into(), id.into()],
    )
}

fn route_probe_update_sharding_stmt(id: i64, payload: String) -> Statement {
    Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "UPDATE {BENCH_SCHEMA}.route_probe SET payload = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2"
        ),
        [payload.into(), id.into()],
    )
}

fn next_existing_id(counter: &AtomicI64, base: i64, size: i64) -> i64 {
    let offset = counter.fetch_add(1, Ordering::Relaxed).rem_euclid(size);
    base + offset
}

fn next_limit_window(counter: &AtomicI64, base: i64, size: i64, width: i64) -> (i64, i64) {
    let max_start = (size - width).max(1);
    let offset = counter
        .fetch_add(width.max(1), Ordering::Relaxed)
        .rem_euclid(max_start);
    let start = base + offset;
    (start, start + width)
}

fn next_insert_id(counter: &AtomicI64) -> i64 {
    counter.fetch_add(1, Ordering::Relaxed)
}

#[cfg(feature = "web")]
#[derive(Debug, Deserialize)]
struct IdQuery {
    id: i64,
}

#[cfg(feature = "web")]
#[derive(Debug, Deserialize)]
struct RangeQuery {
    start: i64,
    end: i64,
}

#[cfg(feature = "web")]
fn build_http_raw_context_router() -> Router {
    Router::new().route("/tenant-context", get(http_raw_context_handler))
}

#[cfg(feature = "web")]
fn build_http_tenant_context_router(sharding: Option<ShardingConnection>) -> Router {
    let mut layer = TenantContextLayer::from_header();
    if let Some(sharding) = sharding {
        layer = layer.with_sharding(sharding);
    }
    Router::new()
        .route("/tenant-context", get(http_tenant_context_handler))
        .layer(layer)
}

#[cfg(feature = "web")]
fn build_http_raw_select_router(raw: DatabaseConnection) -> Router {
    Router::new()
        .route("/tenant/select", get(http_raw_select_handler))
        .layer(Extension(raw))
}

#[cfg(feature = "web")]
fn build_http_manual_bind_select_router(sharding: ShardingConnection) -> Router {
    Router::new()
        .route("/tenant/select", get(http_manual_bind_select_handler))
        .layer(Extension(sharding))
        .layer(TenantContextLayer::from_header())
}

#[cfg(feature = "web")]
fn build_http_auto_inject_select_router(sharding: ShardingConnection) -> Router {
    Router::new()
        .route("/tenant/select", get(http_auto_inject_select_handler))
        .layer(TenantContextLayer::from_header().with_sharding(sharding))
}

#[cfg(feature = "web")]
fn build_http_raw_limit_router(raw: DatabaseConnection) -> Router {
    Router::new()
        .route("/tenant/limit", get(http_raw_limit_handler))
        .layer(Extension(raw))
}

#[cfg(feature = "web")]
fn build_http_manual_bind_limit_router(sharding: ShardingConnection) -> Router {
    Router::new()
        .route("/tenant/limit", get(http_manual_bind_limit_handler))
        .layer(Extension(sharding))
        .layer(TenantContextLayer::from_header())
}

#[cfg(feature = "web")]
fn build_http_auto_inject_limit_router(sharding: ShardingConnection) -> Router {
    Router::new()
        .route("/tenant/limit", get(http_auto_inject_limit_handler))
        .layer(TenantContextLayer::from_header().with_sharding(sharding))
}

#[cfg(feature = "web")]
async fn http_raw_context_handler(headers: HeaderMap) -> impl IntoResponse {
    tenant_id_from_headers(&headers).to_string()
}

#[cfg(feature = "web")]
async fn http_tenant_context_handler(CurrentTenant(tenant): CurrentTenant) -> impl IntoResponse {
    tenant.tenant_id
}

#[cfg(feature = "web")]
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

#[cfg(feature = "web")]
async fn http_manual_bind_select_handler(
    CurrentTenant(tenant): CurrentTenant,
    Extension(sharding): Extension<ShardingConnection>,
    Query(IdQuery { id }): Query<IdQuery>,
) -> impl IntoResponse {
    let row = sharding
        .with_tenant_context(tenant)
        .query_one_raw(tenant_probe_select_by_id_sharding_stmt(id))
        .await
        .expect("manual bind http tenant select")
        .expect("manual bind http tenant row");
    payload_from_row(row)
}

#[cfg(feature = "web")]
async fn http_auto_inject_select_handler(
    CurrentTenant(_tenant): CurrentTenant,
    TenantShardingConnection(sharding): TenantShardingConnection,
    Query(IdQuery { id }): Query<IdQuery>,
) -> impl IntoResponse {
    let row = sharding
        .query_one_raw(tenant_probe_select_by_id_sharding_stmt(id))
        .await
        .expect("auto inject http tenant select")
        .expect("auto inject http tenant row");
    payload_from_row(row)
}

#[cfg(feature = "web")]
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

#[cfg(feature = "web")]
async fn http_manual_bind_limit_handler(
    CurrentTenant(tenant): CurrentTenant,
    Extension(sharding): Extension<ShardingConnection>,
    Query(RangeQuery { start, end }): Query<RangeQuery>,
) -> impl IntoResponse {
    let rows = sharding
        .with_tenant_context(tenant)
        .query_all_raw(tenant_probe_limit_sharding_stmt(start, end))
        .await
        .expect("manual bind http tenant limit");
    rows.len().to_string()
}

#[cfg(feature = "web")]
async fn http_auto_inject_limit_handler(
    CurrentTenant(_tenant): CurrentTenant,
    TenantShardingConnection(sharding): TenantShardingConnection,
    Query(RangeQuery { start, end }): Query<RangeQuery>,
) -> impl IntoResponse {
    let rows = sharding
        .query_all_raw(tenant_probe_limit_sharding_stmt(start, end))
        .await
        .expect("auto inject http tenant limit");
    rows.len().to_string()
}

#[cfg(feature = "web")]
fn tenant_id_from_headers(headers: &HeaderMap) -> &str {
    headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .expect("x-tenant-id header")
}

#[cfg(feature = "web")]
fn payload_from_row(row: QueryResult) -> String {
    row.try_get("", "payload").expect("payload")
}

#[cfg(feature = "web")]
fn http_context_request() -> Request<Body> {
    Request::builder()
        .uri("/tenant-context")
        .header("x-tenant-id", TENANT_A)
        .body(Body::empty())
        .expect("http context request")
}

#[cfg(feature = "web")]
fn http_select_request(id: i64) -> Request<Body> {
    Request::builder()
        .uri(format!("/tenant/select?id={id}"))
        .header("x-tenant-id", TENANT_A)
        .body(Body::empty())
        .expect("http select request")
}

#[cfg(feature = "web")]
fn http_limit_request(start: i64, end: i64) -> Request<Body> {
    Request::builder()
        .uri(format!("/tenant/limit?start={start}&end={end}"))
        .header("x-tenant-id", TENANT_A)
        .body(Body::empty())
        .expect("http limit request")
}

fn route_shard_index(id: i64) -> i64 {
    id.rem_euclid(4)
}

fn passthrough_config(database_url: &str) -> String {
    format!(
        r#"
        [datasources.ds_bench]
        uri = "{database_url}"
        schema = "{schema}"
        role = "primary"

        [sharding.global]
        default_datasource = "ds_bench"
        "#,
        schema = BENCH_SCHEMA,
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

fn sharded_config(database_url: &str) -> String {
    format!(
        r#"
        [datasources.ds_bench]
        uri = "{database_url}"
        schema = "{schema}"
        role = "primary"

        [[sharding.tables]]
        logic_table = "{schema}.route_probe"
        actual_tables = [
            "{schema}.route_probe_0",
            "{schema}.route_probe_1",
            "{schema}.route_probe_2",
            "{schema}.route_probe_3"
        ]
        sharding_column = "id"
        algorithm = "hash_mod"

          [sharding.tables.algorithm_props]
          count = 4

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
    static BENCH_CONFIG_COUNTER: AtomicU64 = AtomicU64::new(1);

    let path = std::env::temp_dir().join(format!(
        "summer-sharding-bench-{}-{}.toml",
        std::process::id(),
        BENCH_CONFIG_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, content).expect("write benchmark temp config");
    path
}

criterion_group!(benches, benchmark_sharding_overhead);
criterion_main!(benches);
