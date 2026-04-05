use chrono::{DateTime, FixedOffset};
use summer::async_trait;

pub type DomainResult<T> = anyhow::Result<T>;

#[derive(Debug, Clone, PartialEq)]
pub struct GuardrailConfigAggregate {
    pub id: i64,
    pub scope_type: String,
    pub organization_id: i64,
    pub project_id: i64,
    pub enabled: bool,
    pub mode: String,
    pub system_rules: serde_json::Value,
    pub allowed_file_types: serde_json::Value,
    pub max_file_size_mb: i32,
    pub pii_action: String,
    pub secret_action: String,
    pub metadata: serde_json::Value,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

#[async_trait]
pub trait GuardrailConfigReadRepository: Send + Sync {
    async fn find_by_id(&self, id: i64) -> DomainResult<Option<GuardrailConfigAggregate>>;
}
