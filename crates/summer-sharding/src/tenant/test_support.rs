use std::sync::OnceLock;

use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use tokio::sync::Mutex;
use url::Url;

use crate::{
    ShardingConnection,
    algorithm::normalize_tenant_suffix,
    error::{Result, ShardingError},
    tenant::{
        SeaOrmTenantMetadataLoader, TenantMetadataLoader, TenantMetadataRecord,
        TenantMetadataSchema,
    },
};

mod tenant_datasource_entity {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(schema_name = "sys", table_name = "tenant_datasource")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub tenant_id: String,
        pub isolation_level: i16,
        pub status: Option<String>,
        pub schema_name: Option<String>,
        pub datasource_name: Option<String>,
        pub db_uri: Option<String>,
        pub db_enable_logging: Option<bool>,
        pub db_min_conns: Option<i32>,
        pub db_max_conns: Option<i32>,
        pub db_connect_timeout_ms: Option<i64>,
        pub db_idle_timeout_ms: Option<i64>,
        pub db_acquire_timeout_ms: Option<i64>,
        pub db_test_before_acquire: Option<bool>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(Debug, Clone, Copy, Default)]
struct TestTenantMetadataSchema;

impl TenantMetadataSchema for TestTenantMetadataSchema {
    type Entity = tenant_datasource_entity::Entity;
    fn into_record(model: tenant_datasource_entity::Model) -> TenantMetadataRecord {
        let isolation_level = match model.isolation_level {
            1 => crate::config::TenantIsolationLevel::SharedRow,
            2 => crate::config::TenantIsolationLevel::SeparateTable,
            3 => crate::config::TenantIsolationLevel::SeparateSchema,
            4 => crate::config::TenantIsolationLevel::SeparateDatabase,
            _ => crate::config::TenantIsolationLevel::SharedRow,
        };

        TenantMetadataRecord {
            tenant_id: model.tenant_id,
            isolation_level,
            status: model.status,
            schema_name: model.schema_name,
            datasource_name: model.datasource_name,
            db_uri: model.db_uri,
            db_enable_logging: model.db_enable_logging,
            db_min_conns: model
                .db_min_conns
                .and_then(|value| u32::try_from(value).ok()),
            db_max_conns: model
                .db_max_conns
                .and_then(|value| u32::try_from(value).ok()),
            db_connect_timeout_ms: model
                .db_connect_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_idle_timeout_ms: model
                .db_idle_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_acquire_timeout_ms: model
                .db_acquire_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_test_before_acquire: model.db_test_before_acquire,
        }
    }
}

pub(crate) fn register_test_metadata_loader(connection: &ShardingConnection) {
    let loader: std::sync::Arc<dyn TenantMetadataLoader> =
        std::sync::Arc::new(SeaOrmTenantMetadataLoader::<TestTenantMetadataSchema>::new());
    connection.set_metadata_loader(loader);
}

const DEFAULT_E2E_DATABASE_URL: &str =
    "postgres://admin:123456@localhost/summerrs-admin?options=-c%20TimeZone%3DAsia%2FShanghai";
const DEFAULT_E2E_REPLICA_DATABASE_URL: &str = "postgres://admin:123456@localhost/summerrs_admin_sharding_e2e?options=-c%20TimeZone%3DAsia%2FShanghai";

fn prepare_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn e2e_database_url() -> String {
    std::env::var("SUMMER_SHARDING_E2E_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| DEFAULT_E2E_DATABASE_URL.to_string())
}

pub(crate) fn e2e_replica_database_url() -> String {
    std::env::var("SUMMER_SHARDING_E2E_REPLICA_DATABASE_URL")
        .unwrap_or_else(|_| DEFAULT_E2E_REPLICA_DATABASE_URL.to_string())
}

pub(crate) async fn prepare_probe_e2e_environment(
    primary_url: &str,
    separate_database_url: &str,
) -> Result<()> {
    let _guard = prepare_lock().lock().await;
    ensure_database_exists(primary_url).await?;
    ensure_tenant_datasource_table(primary_url).await?;
    seed_shared_row_probe(primary_url).await?;
    seed_schema_probe(primary_url).await?;
    seed_table_probe(primary_url, "T-SEED-TABLE").await?;
    ensure_database_exists(separate_database_url).await?;
    seed_database_probe(separate_database_url).await?;
    upsert_probe_tenant_metadata(primary_url, separate_database_url).await?;
    Ok(())
}

pub(crate) async fn prepare_rw_probe_environment(
    primary_url: &str,
    replica_url: &str,
) -> Result<()> {
    let _guard = prepare_lock().lock().await;
    ensure_database_exists(primary_url).await?;
    ensure_database_exists(replica_url).await?;
    let primary = Database::connect(primary_url).await?;
    let replica = Database::connect(replica_url).await?;

    for connection in [&primary, &replica] {
        connection
            .execute_unprepared(
                r#"
                CREATE SCHEMA IF NOT EXISTS test;
                CREATE TABLE IF NOT EXISTS test.rw_probe (
                    id BIGINT PRIMARY KEY,
                    payload VARCHAR(255) NOT NULL
                );
                "#,
            )
            .await?;
    }

    primary
        .execute_unprepared(
            r#"
            DELETE FROM test.rw_probe;
            INSERT INTO test.rw_probe (id, payload)
            VALUES (1, 'primary-read')
            ON CONFLICT (id) DO UPDATE SET payload = EXCLUDED.payload;
            "#,
        )
        .await?;
    replica
        .execute_unprepared(
            r#"
            DELETE FROM test.rw_probe;
            INSERT INTO test.rw_probe (id, payload)
            VALUES (1, 'replica-read')
            ON CONFLICT (id) DO UPDATE SET payload = EXCLUDED.payload;
            "#,
        )
        .await?;
    Ok(())
}

pub(crate) async fn prepare_shadow_probe_environment(primary_url: &str) -> Result<()> {
    let _guard = prepare_lock().lock().await;
    ensure_database_exists(primary_url).await?;
    let connection = Database::connect(primary_url).await?;
    connection
        .execute_unprepared(
            r#"
            CREATE SCHEMA IF NOT EXISTS test;
            CREATE TABLE IF NOT EXISTS test.shadow_probe (
                id BIGINT PRIMARY KEY,
                payload VARCHAR(255) NOT NULL
            );
            CREATE TABLE IF NOT EXISTS test.shadow_probe_shadow (
                id BIGINT PRIMARY KEY,
                payload VARCHAR(255) NOT NULL
            );

            DELETE FROM test.shadow_probe;
            DELETE FROM test.shadow_probe_shadow;

            INSERT INTO test.shadow_probe (id, payload)
            VALUES (1, 'base-row')
            ON CONFLICT (id) DO UPDATE SET payload = EXCLUDED.payload;

            INSERT INTO test.shadow_probe_shadow (id, payload)
            VALUES (1, 'shadow-row')
            ON CONFLICT (id) DO UPDATE SET payload = EXCLUDED.payload;
            "#,
        )
        .await?;
    Ok(())
}

async fn ensure_tenant_datasource_table(database_url: &str) -> Result<()> {
    let connection = Database::connect(database_url).await?;
    connection
        .execute_unprepared(
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

async fn seed_shared_row_probe(database_url: &str) -> Result<()> {
    let connection = Database::connect(database_url).await?;
    connection
        .execute_unprepared(
            r#"
            CREATE SCHEMA IF NOT EXISTS test;
            CREATE TABLE IF NOT EXISTS test.tenant_probe (
                id BIGINT PRIMARY KEY,
                tenant_id VARCHAR(64) NOT NULL,
                payload VARCHAR(255) NOT NULL
            );

            DELETE FROM test.tenant_probe;

            INSERT INTO test.tenant_probe (id, tenant_id, payload) VALUES
                (1, 'T-E2E-A', 'alpha-1'),
                (2, 'T-E2E-A', 'alpha-2'),
                (3, 'T-E2E-B', 'beta-1');
            "#,
        )
        .await?;
    Ok(())
}

async fn seed_schema_probe(database_url: &str) -> Result<()> {
    let connection = Database::connect(database_url).await?;
    connection
        .execute_unprepared(
            r#"
            CREATE SCHEMA IF NOT EXISTS tenant_seed_schema;
            CREATE TABLE IF NOT EXISTS tenant_seed_schema.tenant_probe_isolated (
                id BIGINT PRIMARY KEY,
                payload VARCHAR(255) NOT NULL
            );

            DELETE FROM tenant_seed_schema.tenant_probe_isolated;

            INSERT INTO tenant_seed_schema.tenant_probe_isolated (id, payload)
            VALUES (1, 'schema-row-1')
            ON CONFLICT (id) DO UPDATE SET payload = EXCLUDED.payload;
            "#,
        )
        .await?;
    Ok(())
}

async fn seed_table_probe(database_url: &str, tenant_id: &str) -> Result<()> {
    let connection = Database::connect(database_url).await?;
    let table_name = format!(
        "test.tenant_probe_isolated_{}",
        normalize_tenant_suffix(tenant_id)
    );
    connection
        .execute_unprepared(
            format!(
                r#"
                CREATE SCHEMA IF NOT EXISTS test;
                CREATE TABLE IF NOT EXISTS {table_name} (
                    id BIGINT PRIMARY KEY,
                    payload VARCHAR(255) NOT NULL
                );

                DELETE FROM {table_name};

                INSERT INTO {table_name} (id, payload)
                VALUES (1, 'table-row-1')
                ON CONFLICT (id) DO UPDATE SET payload = EXCLUDED.payload;
                "#
            )
            .as_str(),
        )
        .await?;
    Ok(())
}

async fn seed_database_probe(database_url: &str) -> Result<()> {
    let connection = Database::connect(database_url).await?;
    connection
        .execute_unprepared(
            r#"
            CREATE SCHEMA IF NOT EXISTS test;
            CREATE TABLE IF NOT EXISTS test.tenant_probe_isolated (
                id BIGINT PRIMARY KEY,
                payload VARCHAR(255) NOT NULL
            );

            DELETE FROM test.tenant_probe_isolated;

            INSERT INTO test.tenant_probe_isolated (id, payload)
            VALUES (1, 'db-row-1')
            ON CONFLICT (id) DO UPDATE SET payload = EXCLUDED.payload;
            "#,
        )
        .await?;
    Ok(())
}

async fn upsert_probe_tenant_metadata(
    primary_url: &str,
    separate_database_url: &str,
) -> Result<()> {
    let connection = Database::connect(primary_url).await?;
    let database_url = escape_literal(separate_database_url);
    connection
        .execute_unprepared(
            format!(
                r#"
                DELETE FROM sys.tenant_datasource
                WHERE tenant_id IN ('T-SEED-SCHEMA', 'T-SEED-TABLE', 'T-SEED-DB');

                INSERT INTO sys.tenant_datasource (
                    tenant_id,
                    isolation_level,
                    status,
                    schema_name,
                    datasource_name,
                    db_uri,
                    db_enable_logging,
                    db_min_conns,
                    db_max_conns,
                    db_connect_timeout_ms,
                    db_idle_timeout_ms,
                    db_acquire_timeout_ms,
                    db_test_before_acquire,
                    readonly_config,
                    extra_config,
                    remark,
                    create_by,
                    update_by
                ) VALUES
                    (
                        'T-SEED-SCHEMA',
                        3,
                        'active',
                        'tenant_seed_schema',
                        NULL,
                        NULL,
                        false,
                        1,
                        10,
                        NULL,
                        NULL,
                        NULL,
                        true,
                        '{{}}'::jsonb,
                        '{{}}'::jsonb,
                        'seed',
                        'test',
                        'test'
                    ),
                    (
                        'T-SEED-TABLE',
                        2,
                        'active',
                        NULL,
                        NULL,
                        NULL,
                        false,
                        1,
                        10,
                        NULL,
                        NULL,
                        NULL,
                        true,
                        '{{}}'::jsonb,
                        '{{}}'::jsonb,
                        'seed',
                        'test',
                        'test'
                    ),
                    (
                        'T-SEED-DB',
                        4,
                        'active',
                        NULL,
                        'tenant_tseeddb',
                        '{database_url}',
                        false,
                        1,
                        10,
                        NULL,
                        NULL,
                        NULL,
                        true,
                        '{{}}'::jsonb,
                        '{{}}'::jsonb,
                        'seed',
                        'test',
                        'test'
                    )
                ON CONFLICT (tenant_id) DO UPDATE SET
                    isolation_level = EXCLUDED.isolation_level,
                    status = EXCLUDED.status,
                    schema_name = EXCLUDED.schema_name,
                    datasource_name = EXCLUDED.datasource_name,
                    db_uri = EXCLUDED.db_uri,
                    db_enable_logging = EXCLUDED.db_enable_logging,
                    db_min_conns = EXCLUDED.db_min_conns,
                    db_max_conns = EXCLUDED.db_max_conns,
                    db_connect_timeout_ms = EXCLUDED.db_connect_timeout_ms,
                    db_idle_timeout_ms = EXCLUDED.db_idle_timeout_ms,
                    db_acquire_timeout_ms = EXCLUDED.db_acquire_timeout_ms,
                    db_test_before_acquire = EXCLUDED.db_test_before_acquire,
                    readonly_config = EXCLUDED.readonly_config,
                    extra_config = EXCLUDED.extra_config,
                    remark = EXCLUDED.remark,
                    update_by = EXCLUDED.update_by,
                    update_time = CURRENT_TIMESTAMP;
                "#
            )
            .as_str(),
        )
        .await?;
    Ok(())
}

async fn ensure_database_exists(database_url: &str) -> Result<()> {
    let database_name = database_name(database_url)?;
    if database_name == "postgres" {
        return Ok(());
    }

    let admin_url = admin_database_url(database_url)?;
    let connection = Database::connect(admin_url.as_str()).await?;
    let exists = connection
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 AS exists FROM pg_database WHERE datname = $1",
            [database_name.clone().into()],
        ))
        .await?;

    if exists.is_none() {
        connection
            .execute_unprepared(format!("CREATE DATABASE {}", quote_ident(&database_name)).as_str())
            .await?;
    }

    Ok(())
}

fn admin_database_url(database_url: &str) -> Result<String> {
    let mut url = Url::parse(database_url).map_err(|error| {
        ShardingError::Config(format!("invalid database url `{database_url}`: {error}"))
    })?;
    url.set_path("/postgres");
    Ok(url.to_string())
}

fn database_name(database_url: &str) -> Result<String> {
    let url = Url::parse(database_url).map_err(|error| {
        ShardingError::Config(format!("invalid database url `{database_url}`: {error}"))
    })?;
    url.path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
        .map(|segment| segment.to_string())
        .ok_or_else(|| {
            ShardingError::Config(format!(
                "database url `{database_url}` missing database name"
            ))
        })
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn escape_literal(value: &str) -> String {
    value.replace('\'', "''")
}
