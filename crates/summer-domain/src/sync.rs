use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    Create,
    Update,
    Noop,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncChange {
    pub target: String,
    pub key: String,
    pub action: SyncAction,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub create_count: u64,
    pub update_count: u64,
    pub noop_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub summary: SyncSummary,
    pub changes: Vec<SyncChange>,
}

impl SyncPlan {
    pub fn new(changes: Vec<SyncChange>) -> Self {
        let mut summary = SyncSummary::default();
        for change in &changes {
            match change.action {
                SyncAction::Create => summary.create_count += 1,
                SyncAction::Update => summary.update_count += 1,
                SyncAction::Noop => summary.noop_count += 1,
            }
        }
        Self { summary, changes }
    }
}
