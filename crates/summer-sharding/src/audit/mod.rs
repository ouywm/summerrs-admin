mod log;

use crate::router::RoutePlan;

pub use log::DefaultSqlAuditor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub sql: String,
    pub duration_ms: u128,
    pub route: RoutePlan,
    pub is_slow_query: bool,
    pub full_scatter: bool,
    pub missing_sharding_key: bool,
}

pub trait SqlAuditor: Send + Sync + 'static {
    fn record(&self, event: AuditEvent);
}
