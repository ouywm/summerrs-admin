// TODO: 当前项目统一采用 SQL 改写做租户隔离，RLS 暂不接入运行链路与测试体系。
#[derive(Debug, Clone, Default)]
pub struct TenantRlsManager;

impl TenantRlsManager {
    pub fn enable_policy_sql(
        &self,
        table: &str,
        policy_name: &str,
        tenant_column: &str,
    ) -> Vec<String> {
        vec![
            format!("ALTER TABLE {table} ENABLE ROW LEVEL SECURITY"),
            format!(
                "CREATE POLICY {policy_name} ON {table} USING ({tenant_column} = current_setting('app.current_tenant'))"
            ),
        ]
    }

    pub fn disable_policy_sql(&self, table: &str, policy_name: &str) -> Vec<String> {
        vec![
            format!("DROP POLICY IF EXISTS {policy_name} ON {table}"),
            format!("ALTER TABLE {table} DISABLE ROW LEVEL SECURITY"),
        ]
    }

    pub fn set_current_tenant_sql(&self, tenant_id: &str) -> String {
        format!(
            "SET LOCAL app.current_tenant = '{}'",
            tenant_id.replace('\'', "''")
        )
    }
}
