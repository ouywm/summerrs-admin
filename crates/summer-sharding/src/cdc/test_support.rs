use std::{process::Command, time::Duration};

use rand::random;
use reqwest::Client;
use sea_orm::{ConnectionTrait, Database};

use crate::error::{Result, ShardingError};

const TEST_DB_USER: &str = "admin";
const TEST_DB_PASSWORD: &str = "123456";
const TEST_DB_NAME: &str = "summer_sharding_cdc_e2e";
const TEST_IMAGE: &str = "postgres:16-alpine";
const CLICKHOUSE_IMAGE: &str = "clickhouse/clickhouse-server:24.8";
const CLICKHOUSE_USER: &str = "admin";
const CLICKHOUSE_PASSWORD: &str = "123456";

#[derive(Debug)]
pub(crate) struct LogicalReplicationTestDatabase {
    database_url: String,
    container_name: Option<String>,
}

impl LogicalReplicationTestDatabase {
    pub(crate) async fn start() -> Result<Self> {
        if let Ok(database_url) = std::env::var("SUMMER_SHARDING_CDC_E2E_DATABASE_URL") {
            wait_until_database_ready(database_url.as_str()).await?;
            return Ok(Self {
                database_url,
                container_name: None,
            });
        }

        let container_name = format!(
            "summer-sharding-cdc-e2e-{}-{}",
            std::process::id(),
            random::<u32>()
        );

        run_docker(["rm", "-f", container_name.as_str()]).ok();
        run_docker([
            "run",
            "-d",
            "--name",
            container_name.as_str(),
            "-e",
            "POSTGRES_USER=admin",
            "-e",
            "POSTGRES_PASSWORD=123456",
            "-e",
            "POSTGRES_DB=summer_sharding_cdc_e2e",
            "-p",
            "127.0.0.1::5432",
            TEST_IMAGE,
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "max_replication_slots=10",
            "-c",
            "max_wal_senders=10",
        ])?;
        let port = docker_host_port(container_name.as_str(), "5432/tcp")?;
        let database_url =
            format!("postgres://{TEST_DB_USER}:{TEST_DB_PASSWORD}@127.0.0.1:{port}/{TEST_DB_NAME}");

        wait_until_database_ready(database_url.as_str()).await?;

        Ok(Self {
            database_url,
            container_name: Some(container_name),
        })
    }

    pub(crate) fn database_url(&self) -> &str {
        self.database_url.as_str()
    }
}

impl Drop for LogicalReplicationTestDatabase {
    fn drop(&mut self) {
        if let Some(container_name) = &self.container_name {
            let _ = run_docker(["rm", "-f", container_name.as_str()]);
        }
    }
}

#[derive(Debug)]
pub(crate) struct ClickHouseTestServer {
    http_url: String,
    container_name: Option<String>,
}

impl ClickHouseTestServer {
    pub(crate) async fn start() -> Result<Self> {
        if let Ok(http_url) = std::env::var("SUMMER_SHARDING_CLICKHOUSE_E2E_URL") {
            wait_until_clickhouse_ready(http_url.as_str()).await?;
            return Ok(Self {
                http_url,
                container_name: None,
            });
        }

        let container_name = format!(
            "summer-sharding-clickhouse-e2e-{}-{}",
            std::process::id(),
            random::<u32>()
        );

        run_docker(["rm", "-f", container_name.as_str()]).ok();
        run_docker([
            "run",
            "-d",
            "--name",
            container_name.as_str(),
            "-e",
            "CLICKHOUSE_USER=admin",
            "-e",
            "CLICKHOUSE_PASSWORD=123456",
            "-e",
            "CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1",
            "-p",
            "127.0.0.1::8123",
            CLICKHOUSE_IMAGE,
        ])?;
        let port = docker_host_port(container_name.as_str(), "8123/tcp")?;
        let http_url = format!("http://{CLICKHOUSE_USER}:{CLICKHOUSE_PASSWORD}@127.0.0.1:{port}");

        wait_until_clickhouse_ready(http_url.as_str()).await?;

        Ok(Self {
            http_url,
            container_name: Some(container_name),
        })
    }

    pub(crate) fn http_url(&self) -> &str {
        self.http_url.as_str()
    }
}

impl Drop for ClickHouseTestServer {
    fn drop(&mut self) {
        if let Some(container_name) = &self.container_name {
            let _ = run_docker(["rm", "-f", container_name.as_str()]);
        }
    }
}

#[derive(Debug)]
pub(crate) struct PreparedTransactionTestDatabases {
    primary_database_url: String,
    secondary_database_url: String,
    container_name: Option<String>,
}

impl PreparedTransactionTestDatabases {
    pub(crate) async fn start() -> Result<Self> {
        if let (Ok(primary_database_url), Ok(secondary_database_url)) = (
            std::env::var("SUMMER_SHARDING_2PC_E2E_DATABASE_URL_A"),
            std::env::var("SUMMER_SHARDING_2PC_E2E_DATABASE_URL_B"),
        ) {
            wait_until_database_ready(primary_database_url.as_str()).await?;
            wait_until_database_ready(secondary_database_url.as_str()).await?;
            return Ok(Self {
                primary_database_url,
                secondary_database_url,
                container_name: None,
            });
        }

        let container_name = format!(
            "summer-sharding-2pc-e2e-{}-{}",
            std::process::id(),
            random::<u32>()
        );
        let primary_db = "summer_sharding_2pc_a";
        let secondary_db = "summer_sharding_2pc_b";

        run_docker(["rm", "-f", container_name.as_str()]).ok();
        run_docker([
            "run",
            "-d",
            "--name",
            container_name.as_str(),
            "-e",
            "POSTGRES_USER=admin",
            "-e",
            "POSTGRES_PASSWORD=123456",
            "-e",
            "POSTGRES_DB=summer_sharding_2pc_a",
            "-p",
            "127.0.0.1::5432",
            TEST_IMAGE,
            "postgres",
            "-c",
            "max_prepared_transactions=32",
        ])?;
        let port = docker_host_port(container_name.as_str(), "5432/tcp")?;
        let primary_database_url =
            format!("postgres://{TEST_DB_USER}:{TEST_DB_PASSWORD}@127.0.0.1:{port}/{primary_db}");
        let secondary_database_url =
            format!("postgres://{TEST_DB_USER}:{TEST_DB_PASSWORD}@127.0.0.1:{port}/{secondary_db}");

        wait_until_database_ready(primary_database_url.as_str()).await?;
        let primary = Database::connect(primary_database_url.as_str()).await?;
        primary
            .execute_unprepared(format!("CREATE DATABASE {secondary_db}").as_str())
            .await?;
        wait_until_database_ready(secondary_database_url.as_str()).await?;

        Ok(Self {
            primary_database_url,
            secondary_database_url,
            container_name: Some(container_name),
        })
    }

    pub(crate) fn primary_database_url(&self) -> &str {
        self.primary_database_url.as_str()
    }

    pub(crate) fn secondary_database_url(&self) -> &str {
        self.secondary_database_url.as_str()
    }
}

impl Drop for PreparedTransactionTestDatabases {
    fn drop(&mut self) {
        if let Some(container_name) = &self.container_name {
            let _ = run_docker(["rm", "-f", container_name.as_str()]);
        }
    }
}

#[derive(Debug)]
pub(crate) struct PrimaryReplicaTestCluster {
    primary_database_url: String,
    replica_database_url: String,
    network_name: Option<String>,
    primary_container_name: Option<String>,
    replica_container_name: Option<String>,
    replica_volume_name: Option<String>,
}

impl PrimaryReplicaTestCluster {
    pub(crate) async fn start() -> Result<Self> {
        let suffix = format!("{}-{}", std::process::id(), random::<u32>());
        let network_name = format!("summer-sharding-rw-net-{suffix}");
        let primary_container_name = format!("summer-sharding-rw-primary-{suffix}");
        let replica_container_name = format!("summer-sharding-rw-replica-{suffix}");
        let replica_volume_name = format!("summer-sharding-rw-replica-data-{suffix}");
        let database_name = "summer_sharding_rw_e2e";

        run_docker(["rm", "-f", primary_container_name.as_str()]).ok();
        run_docker(["rm", "-f", replica_container_name.as_str()]).ok();
        run_docker(["network", "rm", network_name.as_str()]).ok();
        run_docker(["volume", "rm", "-f", replica_volume_name.as_str()]).ok();

        run_docker(["network", "create", network_name.as_str()])?;
        run_docker(["volume", "create", replica_volume_name.as_str()])?;
        run_docker([
            "run",
            "-d",
            "--name",
            primary_container_name.as_str(),
            "--network",
            network_name.as_str(),
            "-e",
            "POSTGRES_USER=admin",
            "-e",
            "POSTGRES_PASSWORD=123456",
            "-e",
            "POSTGRES_DB=summer_sharding_rw_e2e",
            "-p",
            "127.0.0.1::5432",
            TEST_IMAGE,
            "postgres",
            "-c",
            "wal_level=replica",
            "-c",
            "max_wal_senders=10",
            "-c",
            "max_replication_slots=10",
            "-c",
            "hot_standby=on",
        ])?;
        let primary_port = docker_host_port(primary_container_name.as_str(), "5432/tcp")?;
        let primary_database_url = format!(
            "postgres://{TEST_DB_USER}:{TEST_DB_PASSWORD}@127.0.0.1:{primary_port}/{database_name}"
        );
        wait_until_database_ready(primary_database_url.as_str()).await?;

        let replication_setup = format!(
            "echo \"host replication replicator all scram-sha-256\" >> \"$PGDATA/pg_hba.conf\" \
             && psql -U admin -d {database_name} -c \"CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD '123456';\" \
             && psql -U admin -d {database_name} -c \"SELECT pg_reload_conf();\""
        );
        run_docker([
            "exec",
            primary_container_name.as_str(),
            "sh",
            "-lc",
            replication_setup.as_str(),
        ])?;

        let replica_mount = format!("{replica_volume_name}:/var/lib/postgresql/data");
        let basebackup = format!(
            "rm -rf /var/lib/postgresql/data/* \
             && until pg_basebackup -h {primary_container_name} -D /var/lib/postgresql/data -U replicator -Fp -Xs -P -R; do sleep 1; done \
             && echo \"hot_standby = on\" >> /var/lib/postgresql/data/postgresql.auto.conf"
        );
        run_docker([
            "run",
            "--rm",
            "--network",
            network_name.as_str(),
            "-e",
            "PGPASSWORD=123456",
            "-v",
            replica_mount.as_str(),
            TEST_IMAGE,
            "sh",
            "-lc",
            basebackup.as_str(),
        ])?;

        run_docker([
            "run",
            "-d",
            "--name",
            replica_container_name.as_str(),
            "--network",
            network_name.as_str(),
            "-v",
            replica_mount.as_str(),
            "-p",
            "127.0.0.1::5432",
            TEST_IMAGE,
            "postgres",
            "-c",
            "hot_standby=on",
        ])?;
        let replica_port = docker_host_port(replica_container_name.as_str(), "5432/tcp")?;
        let replica_database_url = format!(
            "postgres://{TEST_DB_USER}:{TEST_DB_PASSWORD}@127.0.0.1:{replica_port}/{database_name}"
        );
        wait_until_database_ready(replica_database_url.as_str()).await?;

        Ok(Self {
            primary_database_url,
            replica_database_url,
            network_name: Some(network_name),
            primary_container_name: Some(primary_container_name),
            replica_container_name: Some(replica_container_name),
            replica_volume_name: Some(replica_volume_name),
        })
    }

    pub(crate) fn primary_database_url(&self) -> &str {
        self.primary_database_url.as_str()
    }

    pub(crate) fn replica_database_url(&self) -> &str {
        self.replica_database_url.as_str()
    }

    pub(crate) async fn seed_rw_probe(&self) -> Result<()> {
        let primary = Database::connect(self.primary_database_url.as_str()).await?;
        primary
            .execute_unprepared(
                r#"
                CREATE SCHEMA IF NOT EXISTS test;
                DROP TABLE IF EXISTS test.rw_failover_probe;
                CREATE TABLE test.rw_failover_probe (
                    id BIGINT PRIMARY KEY,
                    payload VARCHAR(255) NOT NULL
                );
                INSERT INTO test.rw_failover_probe(id, payload) VALUES (1, 'replicated-row');
                "#,
            )
            .await?;
        self.wait_for_replica_row_count("test.rw_failover_probe", 1)
            .await
    }

    pub(crate) async fn promote_replica_and_stop_primary(&self) -> Result<()> {
        let replica = Database::connect(self.replica_database_url.as_str()).await?;
        replica
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Postgres,
                "SELECT pg_promote()",
            ))
            .await?;
        for _ in 0..60 {
            let row = replica
                .query_one_raw(sea_orm::Statement::from_string(
                    sea_orm::DbBackend::Postgres,
                    "SELECT pg_is_in_recovery() AS in_recovery",
                ))
                .await?;
            if let Some(row) = row {
                let in_recovery = row.try_get::<bool>("", "in_recovery")?;
                if !in_recovery {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if let Some(primary_container_name) = &self.primary_container_name {
            run_docker(["rm", "-f", primary_container_name.as_str()])?;
        }
        Ok(())
    }

    async fn wait_for_replica_row_count(&self, table: &str, expected: i64) -> Result<()> {
        let replica = Database::connect(self.replica_database_url.as_str()).await?;
        for _ in 0..60 {
            if let Ok(Some(row)) = replica
                .query_one_raw(sea_orm::Statement::from_string(
                    sea_orm::DbBackend::Postgres,
                    format!("SELECT COUNT(*) AS count FROM {table}"),
                ))
                .await
            {
                let count = row.try_get::<i64>("", "count")?;
                if count == expected {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(ShardingError::Route(format!(
            "replica row count for `{table}` did not reach {expected}"
        )))
    }
}

impl Drop for PrimaryReplicaTestCluster {
    fn drop(&mut self) {
        if let Some(primary_container_name) = &self.primary_container_name {
            let _ = run_docker(["rm", "-f", primary_container_name.as_str()]);
        }
        if let Some(replica_container_name) = &self.replica_container_name {
            let _ = run_docker(["rm", "-f", replica_container_name.as_str()]);
        }
        if let Some(network_name) = &self.network_name {
            let _ = run_docker(["network", "rm", network_name.as_str()]);
        }
        if let Some(replica_volume_name) = &self.replica_volume_name {
            let _ = run_docker(["volume", "rm", "-f", replica_volume_name.as_str()]);
        }
    }
}

async fn wait_until_database_ready(database_url: &str) -> Result<()> {
    for _ in 0..60 {
        if Database::connect(database_url).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(ShardingError::Route(format!(
        "logical replication test database at `{database_url}` did not become ready"
    )))
}

async fn wait_until_clickhouse_ready(http_url: &str) -> Result<()> {
    let client = Client::new();
    for _ in 0..60 {
        if let Ok(response) = client.get(format!("{http_url}/ping")).send().await
            && response.status().is_success()
        {
            let body = response.text().await.unwrap_or_default();
            if body.trim().eq_ignore_ascii_case("Ok.") || body.trim().eq_ignore_ascii_case("Ok") {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(ShardingError::Route(format!(
        "clickhouse test server at `{http_url}` did not become ready"
    )))
}

fn run_docker<'a, I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = &'a str>,
{
    run_docker_output(args).map(|_| ())
}

fn run_docker_output<'a, I>(args: I) -> Result<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let output = Command::new("docker").args(args).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(ShardingError::Route(format!(
            "docker command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

fn docker_host_port(container_name: &str, container_port: &str) -> Result<u16> {
    for _ in 0..30 {
        if let Ok(output) = run_docker_output(["port", container_name, container_port])
            && let Some(port) = output
                .rsplit(':')
                .next()
                .and_then(|value| value.parse().ok())
        {
            return Ok(port);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(ShardingError::Route(format!(
        "docker port mapping for `{container_name}` `{container_port}` was not visible"
    )))
}
