use crate::{
    algorithm::normalize_tenant_suffix,
    config::TenantIsolationLevel,
    tenant::{TenantMetadataEvent, TenantMetadataEventKind, TenantMetadataRecord},
};

const METADATA_CHANNEL: &str = "summer_sharding_tenant_metadata";
const RELOAD_FALLBACK_PAYLOAD: &str = r#"{"event":"reload"}"#;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TenantLifecyclePlan {
    pub resource_sql: Vec<String>,
    pub notify_sql: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TenantLifecycleManager;

impl TenantLifecycleManager {
    pub fn plan_onboard(
        &self,
        record: &TenantMetadataRecord,
        base_tables: &[String],
    ) -> TenantLifecyclePlan {
        let naming = TenantNaming::from_record(record);
        TenantLifecyclePlan {
            resource_sql: naming.onboard_sql(record.isolation_level, base_tables),
            notify_sql: vec![pg_notify_sql(upsert_event(record))],
        }
    }

    pub fn plan_offboard(
        &self,
        record: &TenantMetadataRecord,
        base_tables: &[String],
    ) -> TenantLifecyclePlan {
        let naming = TenantNaming::from_record(record);
        TenantLifecyclePlan {
            resource_sql: naming.offboard_sql(record.isolation_level, base_tables),
            notify_sql: vec![pg_notify_sql(delete_event(&record.tenant_id))],
        }
    }

    pub fn onboard_sql(
        &self,
        tenant_id: &str,
        isolation_level: TenantIsolationLevel,
        schema_name: Option<&str>,
    ) -> Vec<String> {
        self.plan_onboard(
            &bare_record(tenant_id, isolation_level, schema_name, "active"),
            &[],
        )
        .resource_sql
    }

    pub fn offboard_sql(
        &self,
        tenant_id: &str,
        isolation_level: TenantIsolationLevel,
        schema_name: Option<&str>,
    ) -> Vec<String> {
        self.plan_offboard(
            &bare_record(tenant_id, isolation_level, schema_name, "inactive"),
            &[],
        )
        .resource_sql
    }
}

struct TenantNaming {
    tenant_suffix: String,
    schema_name: String,
    database_name: String,
}

impl TenantNaming {
    fn from_record(record: &TenantMetadataRecord) -> Self {
        let tenant_suffix = normalize_tenant_suffix(&record.tenant_id);
        let schema_name = record
            .schema_name
            .clone()
            .unwrap_or_else(|| format!("tenant_{tenant_suffix}"));
        let datasource_fallback = record
            .datasource_name
            .clone()
            .unwrap_or_else(|| format!("tenant_{tenant_suffix}"));
        let database_name = resolve_database_name(record, &datasource_fallback);
        Self {
            tenant_suffix,
            schema_name,
            database_name,
        }
    }

    fn onboard_sql(&self, isolation: TenantIsolationLevel, base_tables: &[String]) -> Vec<String> {
        match isolation {
            TenantIsolationLevel::SharedRow => Vec::new(),
            TenantIsolationLevel::SeparateTable => base_tables
                .iter()
                .map(|table| {
                    let suffix = &self.tenant_suffix;
                    format!(
                        "CREATE TABLE IF NOT EXISTS {table}_{suffix} (LIKE {table} INCLUDING ALL)"
                    )
                })
                .collect(),
            TenantIsolationLevel::SeparateSchema => {
                let schema = &self.schema_name;
                std::iter::once(format!("CREATE SCHEMA IF NOT EXISTS {schema}"))
                    .chain(base_tables.iter().map(|table| {
                        let tail = table.rsplit('.').next().unwrap_or(table.as_str());
                        format!(
                            "CREATE TABLE IF NOT EXISTS {schema}.{tail} (LIKE {table} INCLUDING ALL)"
                        )
                    }))
                    .collect()
            }
            // PostgreSQL does not support `CREATE DATABASE IF NOT EXISTS`; caller handles idempotency.
            TenantIsolationLevel::SeparateDatabase => {
                vec![format!("CREATE DATABASE {}", self.database_name)]
            }
        }
    }

    fn offboard_sql(&self, isolation: TenantIsolationLevel, base_tables: &[String]) -> Vec<String> {
        match isolation {
            TenantIsolationLevel::SharedRow => Vec::new(),
            TenantIsolationLevel::SeparateTable => base_tables
                .iter()
                .map(|table| format!("DROP TABLE IF EXISTS {table}_{}", self.tenant_suffix))
                .collect(),
            TenantIsolationLevel::SeparateSchema => {
                vec![format!(
                    "DROP SCHEMA IF EXISTS {} CASCADE",
                    self.schema_name
                )]
            }
            TenantIsolationLevel::SeparateDatabase => {
                vec![format!("DROP DATABASE IF EXISTS {}", self.database_name)]
            }
        }
    }
}

fn upsert_event(record: &TenantMetadataRecord) -> TenantMetadataEvent {
    TenantMetadataEvent {
        event: TenantMetadataEventKind::Upsert,
        tenant_id: Some(record.tenant_id.clone()),
        record: Some(record.clone()),
    }
}

fn delete_event(tenant_id: &str) -> TenantMetadataEvent {
    TenantMetadataEvent {
        event: TenantMetadataEventKind::Delete,
        tenant_id: Some(tenant_id.to_string()),
        record: None,
    }
}

fn bare_record(
    tenant_id: &str,
    isolation_level: TenantIsolationLevel,
    schema_name: Option<&str>,
    status: &str,
) -> TenantMetadataRecord {
    TenantMetadataRecord {
        tenant_id: tenant_id.to_string(),
        isolation_level,
        status: Some(status.to_string()),
        schema_name: schema_name.map(str::to_string),
        datasource_name: None,
        db_uri: None,
        db_enable_logging: None,
        db_min_conns: None,
        db_max_conns: None,
        db_connect_timeout_ms: None,
        db_idle_timeout_ms: None,
        db_acquire_timeout_ms: None,
        db_test_before_acquire: None,
    }
}

fn pg_notify_sql(event: TenantMetadataEvent) -> String {
    let payload = serde_json::to_string(&event)
        .unwrap_or_else(|_| RELOAD_FALLBACK_PAYLOAD.to_string())
        .replace('\'', "''");
    format!("SELECT pg_notify('{METADATA_CHANNEL}', '{payload}')")
}

fn resolve_database_name(record: &TenantMetadataRecord, fallback: &str) -> String {
    let Some(uri) = record.db_uri.as_deref() else {
        return fallback.replace('-', "_");
    };
    let Ok(parsed) = url::Url::parse(uri) else {
        return fallback.replace('-', "_");
    };
    parsed
        .path_segments()
        .and_then(|mut segments| segments.rfind(|seg| !seg.is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| fallback.replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::TenantLifecyclePlan;
    use crate::{
        config::TenantIsolationLevel,
        tenant::{TenantLifecycleManager, TenantMetadataRecord},
    };

    fn record(
        tenant_id: &str,
        isolation_level: TenantIsolationLevel,
        schema_name: Option<&str>,
    ) -> TenantMetadataRecord {
        TenantMetadataRecord {
            tenant_id: tenant_id.to_string(),
            isolation_level,
            status: Some("active".to_string()),
            schema_name: schema_name.map(str::to_string),
            datasource_name: None,
            db_uri: None,
            db_enable_logging: None,
            db_min_conns: None,
            db_max_conns: None,
            db_connect_timeout_ms: None,
            db_idle_timeout_ms: None,
            db_acquire_timeout_ms: None,
            db_test_before_acquire: None,
        }
    }

    #[test]
    fn lifecycle_plan_keeps_only_resource_and_notify_sql() {
        let plan = TenantLifecyclePlan {
            resource_sql: vec!["CREATE SCHEMA IF NOT EXISTS tenant_demo".to_string()],
            notify_sql: vec![
                "SELECT pg_notify('summer_sharding_tenant_metadata', '{}')".to_string(),
            ],
        };

        assert_eq!(plan.resource_sql.len(), 1);
        assert_eq!(plan.notify_sql.len(), 1);
    }

    #[test]
    fn lifecycle_generates_schema_sql() {
        let manager = TenantLifecycleManager;
        assert_eq!(
            manager.onboard_sql(
                "tenant_a",
                TenantIsolationLevel::SeparateSchema,
                Some("tenant_a")
            ),
            vec!["CREATE SCHEMA IF NOT EXISTS tenant_a".to_string()]
        );
        assert_eq!(
            manager.offboard_sql(
                "tenant_a",
                TenantIsolationLevel::SeparateSchema,
                Some("tenant_a")
            ),
            vec!["DROP SCHEMA IF EXISTS tenant_a CASCADE".to_string()]
        );
    }

    #[test]
    fn lifecycle_plans_table_level_resource_and_notify_sql() {
        let manager = TenantLifecycleManager;
        let plan = manager.plan_onboard(
            &record("T-PRO", TenantIsolationLevel::SeparateTable, None),
            &["ai.log".to_string(), "ai.request".to_string()],
        );

        assert!(
            plan.resource_sql
                .iter()
                .any(|sql| sql.contains("ai.log_tpro"))
        );
        assert!(plan.notify_sql[0].contains("pg_notify"));
    }

    #[test]
    fn lifecycle_uses_database_name_from_db_uri_when_present() {
        let manager = TenantLifecycleManager;
        let mut record = record("T-DB", TenantIsolationLevel::SeparateDatabase, None);
        record.datasource_name = Some("tenant_tdb".to_string());
        record.db_uri =
            Some("postgres://admin:123456@localhost/tenant_real_db?sslmode=disable".to_string());

        let plan = manager.plan_onboard(&record, &[]);

        assert_eq!(plan.resource_sql, vec!["CREATE DATABASE tenant_real_db"]);
        assert!(plan.notify_sql[0].contains("pg_notify"));
    }
}
