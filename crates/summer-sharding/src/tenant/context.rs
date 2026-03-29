use serde::{Deserialize, Serialize};

use crate::config::TenantIsolationLevel;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantContext {
    pub tenant_id: String,
    pub isolation_level: TenantIsolationLevel,
    pub datasource_override: Option<String>,
    pub schema_override: Option<String>,
}

impl TenantContext {
    pub fn new(tenant_id: impl Into<String>, isolation_level: TenantIsolationLevel) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            isolation_level,
            datasource_override: None,
            schema_override: None,
        }
    }
}
