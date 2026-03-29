use parking_lot::Mutex;
use tracing::{info, warn};

use crate::audit::{AuditEvent, SqlAuditor};

#[derive(Debug, Default)]
pub struct DefaultSqlAuditor {
    events: Mutex<Vec<AuditEvent>>,
}

impl DefaultSqlAuditor {
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events.lock().clone()
    }
}

impl SqlAuditor for DefaultSqlAuditor {
    fn record(&self, event: AuditEvent) {
        if event.is_slow_query || event.full_scatter || event.missing_sharding_key {
            warn!(sql = %event.sql, duration_ms = event.duration_ms, "summer-sharding audit warning");
        } else {
            info!(sql = %event.sql, duration_ms = event.duration_ms, "summer-sharding audit");
        }
        self.events.lock().push(event);
    }
}

#[cfg(test)]
mod tests {
    use crate::audit::{AuditEvent, DefaultSqlAuditor, SqlAuditor};
    use crate::router::{RoutePlan, SqlOperation};

    #[test]
    fn auditor_stores_events() {
        let auditor = DefaultSqlAuditor::default();
        auditor.record(AuditEvent {
            sql: "select 1".to_string(),
            duration_ms: 10,
            route: RoutePlan {
                operation: SqlOperation::Select,
                logic_tables: Vec::new(),
                targets: Vec::new(),
                order_by: Vec::new(),
                limit: None,
                offset: None,
                broadcast: false,
            },
            is_slow_query: false,
            full_scatter: false,
            missing_sharding_key: false,
        });
        assert_eq!(auditor.events().len(), 1);
    }
}
