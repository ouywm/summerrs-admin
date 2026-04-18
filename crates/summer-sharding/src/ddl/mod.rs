mod ghost;
mod in_memory;
mod scheduler;

use async_trait::async_trait;

use crate::error::Result;

pub use ghost::{GhostTablePlan, GhostTablePlanner};
pub use in_memory::InMemoryOnlineDdlEngine;
pub use scheduler::DdlScheduler;

pub type DdlTaskId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DdlTaskStatus {
    Pending,
    Snapshot,
    CatchUp,
    CutOver,
    Cleanup,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnlineDdlTask {
    pub ddl: String,
    pub actual_tables: Vec<String>,
    pub concurrency: usize,
    pub batch_size: usize,
    pub status: DdlTaskStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdlShardPlan {
    pub table: String,
    pub ghost_table: String,
    pub old_table: String,
    pub slot: String,
    pub publication: String,
    pub snapshot_statements: Vec<String>,
    pub catch_up_statements: Vec<String>,
    pub cutover_statements: Vec<String>,
    pub cleanup_statements: Vec<String>,
}

impl DdlShardPlan {
    pub fn statement_count(&self) -> usize {
        self.snapshot_statements.len()
            + self.catch_up_statements.len()
            + self.cutover_statements.len()
            + self.cleanup_statements.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdlProgress {
    pub id: DdlTaskId,
    pub status: DdlTaskStatus,
    pub total_tables: usize,
    pub completed_tables: usize,
    pub batch_size: usize,
    pub shard_plans: Vec<DdlShardPlan>,
    pub scheduled_batches: Vec<Vec<String>>,
    pub phase_history: Vec<DdlTaskStatus>,
}

#[async_trait]
pub trait OnlineDdlEngine: Send + Sync + 'static {
    async fn submit(&self, task: OnlineDdlTask) -> Result<DdlTaskId>;
    async fn progress(&self, id: DdlTaskId) -> Result<DdlProgress>;
    async fn cancel(&self, id: DdlTaskId) -> Result<()>;
}
