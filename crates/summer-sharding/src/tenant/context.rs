use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::config::TenantIsolationLevel;

tokio::task_local! {
    pub static CURRENT_TENANT: TenantContext;
}

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

pub async fn with_tenant<F, T>(tenant: TenantContext, future: F) -> T
where
    F: Future<Output = T>,
{
    CURRENT_TENANT.scope(tenant, future).await
}

pub fn current_tenant() -> Option<TenantContext> {
    CURRENT_TENANT.try_with(Clone::clone).ok()
}
