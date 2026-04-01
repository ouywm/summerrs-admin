use crate::{
    algorithm::normalize_tenant_suffix,
    config::TenantIsolationLevel,
    tenant::{TenantMetadataEvent, TenantMetadataEventKind, TenantMetadataRecord},
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TenantLifecyclePlan {
    pub resource_sql: Vec<String>,
    pub metadata_sql: Vec<String>,
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
        let tenant_suffix = normalize_tenant_suffix(record.tenant_id.as_str());
        let schema_name = record
            .schema_name
            .clone()
            .unwrap_or_else(|| format!("tenant_{tenant_suffix}"));
        let datasource_name = record
            .datasource_name
            .clone()
            .unwrap_or_else(|| format!("tenant_{tenant_suffix}"));
        let database_name = database_name(record, datasource_name.as_str());

        let resource_sql = match record.isolation_level {
            TenantIsolationLevel::SharedRow => Vec::new(),
            TenantIsolationLevel::SeparateTable => base_tables
                .iter()
                .map(|table| {
                    format!(
                        "CREATE TABLE IF NOT EXISTS {}_{} (LIKE {} INCLUDING ALL)",
                        table, tenant_suffix, table
                    )
                })
                .collect(),
            TenantIsolationLevel::SeparateSchema => {
                let mut statements = vec![format!("CREATE SCHEMA IF NOT EXISTS {schema_name}")];
                statements.extend(base_tables.iter().map(|table| {
                    let table_name = table.rsplit('.').next().unwrap_or(table.as_str());
                    format!(
                        "CREATE TABLE IF NOT EXISTS {schema_name}.{table_name} (LIKE {table} INCLUDING ALL)"
                    )
                }));
                statements
            }
            TenantIsolationLevel::SeparateDatabase => {
                vec![format!("CREATE DATABASE {database_name}")]
            }
        };

        let metadata_sql = vec![format!(
            "INSERT INTO sys.tenant_datasource (tenant_id, isolation_level, status, schema_name, datasource_name, db_uri, db_enable_logging, db_min_conns, db_max_conns, db_connect_timeout_ms, db_idle_timeout_ms, db_acquire_timeout_ms, db_test_before_acquire) \
             VALUES ('{tenant_id}', {isolation}, 'active', {schema_name}, {datasource_name}, {db_uri}, {db_enable_logging}, {db_min_conns}, {db_max_conns}, {db_connect_timeout_ms}, {db_idle_timeout_ms}, {db_acquire_timeout_ms}, {db_test_before_acquire}) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
               isolation_level = EXCLUDED.isolation_level, status = EXCLUDED.status, \
               schema_name = EXCLUDED.schema_name, datasource_name = EXCLUDED.datasource_name, db_uri = EXCLUDED.db_uri, \
               db_enable_logging = EXCLUDED.db_enable_logging, db_min_conns = EXCLUDED.db_min_conns, db_max_conns = EXCLUDED.db_max_conns, \
               db_connect_timeout_ms = EXCLUDED.db_connect_timeout_ms, db_idle_timeout_ms = EXCLUDED.db_idle_timeout_ms, \
               db_acquire_timeout_ms = EXCLUDED.db_acquire_timeout_ms, db_test_before_acquire = EXCLUDED.db_test_before_acquire",
            tenant_id = escape_literal(record.tenant_id.as_str()),
            isolation = isolation_literal(record.isolation_level),
            schema_name =
                option_literal(Some(schema_name.as_str()).filter(|_| {
                    record.isolation_level == TenantIsolationLevel::SeparateSchema
                })),
            datasource_name =
                option_literal(Some(datasource_name.as_str()).filter(|_| {
                    record.isolation_level == TenantIsolationLevel::SeparateDatabase
                })),
            db_uri = option_literal(record.db_uri.as_deref()),
            db_enable_logging = option_bool_literal(record.db_enable_logging),
            db_min_conns = option_u32_literal(record.db_min_conns),
            db_max_conns = record
                .db_max_conns
                .map(|value| value.to_string())
                .unwrap_or_else(|| "NULL".to_string()),
            db_connect_timeout_ms = option_u64_literal(record.db_connect_timeout_ms),
            db_idle_timeout_ms = option_u64_literal(record.db_idle_timeout_ms),
            db_acquire_timeout_ms = option_u64_literal(record.db_acquire_timeout_ms),
            db_test_before_acquire = option_bool_literal(record.db_test_before_acquire),
        )];

        let notify_sql = vec![notify_sql(TenantMetadataEvent {
            event: TenantMetadataEventKind::Upsert,
            tenant_id: Some(record.tenant_id.clone()),
            record: Some(record.clone()),
        })];

        TenantLifecyclePlan {
            resource_sql,
            metadata_sql,
            notify_sql,
        }
    }

    pub fn plan_offboard(
        &self,
        record: &TenantMetadataRecord,
        base_tables: &[String],
    ) -> TenantLifecyclePlan {
        let tenant_suffix = normalize_tenant_suffix(record.tenant_id.as_str());
        let schema_name = record
            .schema_name
            .clone()
            .unwrap_or_else(|| format!("tenant_{tenant_suffix}"));
        let datasource_name = record
            .datasource_name
            .clone()
            .unwrap_or_else(|| format!("tenant_{tenant_suffix}"));
        let database_name = database_name(record, datasource_name.as_str());

        let resource_sql = match record.isolation_level {
            TenantIsolationLevel::SharedRow => Vec::new(),
            TenantIsolationLevel::SeparateTable => base_tables
                .iter()
                .map(|table| format!("DROP TABLE IF EXISTS {}_{}", table, tenant_suffix))
                .collect(),
            TenantIsolationLevel::SeparateSchema => {
                vec![format!("DROP SCHEMA IF EXISTS {schema_name} CASCADE")]
            }
            TenantIsolationLevel::SeparateDatabase => {
                vec![format!("DROP DATABASE IF EXISTS {database_name}")]
            }
        };

        let metadata_sql = vec![format!(
            "UPDATE sys.tenant_datasource SET status = 'inactive' WHERE tenant_id = '{tenant_id}'",
            tenant_id = escape_literal(record.tenant_id.as_str()),
        )];
        let notify_sql = vec![notify_sql(TenantMetadataEvent {
            event: TenantMetadataEventKind::Delete,
            tenant_id: Some(record.tenant_id.clone()),
            record: None,
        })];

        TenantLifecyclePlan {
            resource_sql,
            metadata_sql,
            notify_sql,
        }
    }

    pub fn onboard_sql(
        &self,
        tenant_id: &str,
        isolation_level: TenantIsolationLevel,
        schema_name: Option<&str>,
    ) -> Vec<String> {
        self.plan_onboard(
            &TenantMetadataRecord {
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
            },
            &Vec::new(),
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
            &TenantMetadataRecord {
                tenant_id: tenant_id.to_string(),
                isolation_level,
                status: Some("inactive".to_string()),
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
            },
            &Vec::new(),
        )
        .resource_sql
    }
}

fn isolation_literal(value: TenantIsolationLevel) -> i16 {
    value.code()
}

fn option_literal(value: Option<&str>) -> String {
    value
        .map(|value| format!("'{}'", escape_literal(value)))
        .unwrap_or_else(|| "NULL".to_string())
}

fn option_bool_literal(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "NULL".to_string())
}

fn option_u32_literal(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "NULL".to_string())
}

fn option_u64_literal(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "NULL".to_string())
}

fn escape_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn notify_sql(event: TenantMetadataEvent) -> String {
    format!(
        "SELECT pg_notify('summer_sharding_tenant_metadata', '{}')",
        serde_json::to_string(&event)
            .unwrap_or_else(|_| "{\"event\":\"reload\"}".to_string())
            .replace('\'', "''")
    )
}

fn database_name(record: &TenantMetadataRecord, fallback_datasource_name: &str) -> String {
    record
        .db_uri
        .as_deref()
        .and_then(|uri| url::Url::parse(uri).ok())
        .and_then(|uri| {
            uri.path_segments()
                .and_then(|segments| {
                    let mut segments = segments;
                    segments.rfind(|segment| !segment.is_empty())
                })
                .map(|segment| segment.to_string())
        })
        .unwrap_or_else(|| fallback_datasource_name.replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use crate::{
        config::TenantIsolationLevel,
        tenant::{TenantLifecycleManager, TenantMetadataRecord},
    };

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
    fn lifecycle_plans_table_level_and_metadata_sql() {
        let manager = TenantLifecycleManager;
        let plan = manager.plan_onboard(
            &TenantMetadataRecord {
                tenant_id: "T-PRO".to_string(),
                isolation_level: TenantIsolationLevel::SeparateTable,
                status: Some("active".to_string()),
                schema_name: None,
                datasource_name: None,
                db_uri: None,
                db_enable_logging: None,
                db_min_conns: None,
                db_max_conns: None,
                db_connect_timeout_ms: None,
                db_idle_timeout_ms: None,
                db_acquire_timeout_ms: None,
                db_test_before_acquire: None,
            },
            &["ai.log".to_string(), "ai.request".to_string()],
        );

        assert!(
            plan.resource_sql
                .iter()
                .any(|sql| sql.contains("ai.log_tpro"))
        );
        assert!(plan.metadata_sql[0].contains("INSERT INTO sys.tenant_datasource"));
        assert!(!plan.metadata_sql[0].contains("'2'"));
        assert!(plan.metadata_sql[0].contains(", 2, 'active'"));
        assert!(plan.notify_sql[0].contains("pg_notify"));
    }

    #[test]
    fn lifecycle_uses_database_name_from_db_uri_when_present() {
        let manager = TenantLifecycleManager;
        let plan = manager.plan_onboard(
            &TenantMetadataRecord {
                tenant_id: "T-DB".to_string(),
                isolation_level: TenantIsolationLevel::SeparateDatabase,
                status: Some("active".to_string()),
                schema_name: None,
                datasource_name: Some("tenant_tdb".to_string()),
                db_uri: Some(
                    "postgres://admin:123456@localhost/tenant_real_db?sslmode=disable".to_string(),
                ),
                db_enable_logging: Some(true),
                db_min_conns: Some(2),
                db_max_conns: Some(8),
                db_connect_timeout_ms: Some(1_500),
                db_idle_timeout_ms: Some(2_500),
                db_acquire_timeout_ms: Some(3_500),
                db_test_before_acquire: Some(false),
            },
            &[],
        );

        assert_eq!(plan.resource_sql, vec!["CREATE DATABASE tenant_real_db"]);
        assert!(plan.metadata_sql[0].contains("db_enable_logging"));
        assert!(plan.metadata_sql[0].contains("db_min_conns"));
        assert!(plan.metadata_sql[0].contains("db_connect_timeout_ms"));
        assert!(plan.metadata_sql[0].contains("db_idle_timeout_ms"));
        assert!(plan.metadata_sql[0].contains("db_acquire_timeout_ms"));
        assert!(plan.metadata_sql[0].contains("db_test_before_acquire"));
        assert!(plan.metadata_sql[0].contains("true"));
        assert!(plan.metadata_sql[0].contains(", 2, 8, 1500, 2500, 3500, false)"));
    }
}
