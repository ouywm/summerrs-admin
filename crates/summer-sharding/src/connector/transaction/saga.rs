use std::path::PathBuf;
use std::sync::Arc;

use rand::random;
use sea_orm::ConnectionTrait;
use serde::{Deserialize, Serialize};

use super::ShardingTransaction;
use crate::error::{Result, ShardingError};

#[async_trait::async_trait]
pub trait SagaContext: Send + Sync {
    async fn execute(&self, sql: &str) -> Result<()>;
}

#[async_trait::async_trait]
impl SagaContext for ShardingTransaction {
    async fn execute(&self, sql: &str) -> Result<()> {
        let transactions = self.transactions.lock().await;
        for transaction in transactions.values() {
            transaction.execute_unprepared(sql).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait SagaStep: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, ctx: &dyn SagaContext) -> Result<()>;
    async fn compensate(&self, ctx: &dyn SagaContext) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SagaRunState {
    pub(super) run_id: String,
    pub(super) completed_steps: Vec<String>,
    pub(super) compensated_steps: Vec<String>,
}

pub(super) trait SagaJournal: Send + Sync {
    fn start_run(&self, run_id: &str) -> Result<()>;
    fn mark_step_completed(&self, run_id: &str, step_name: &str) -> Result<()>;
    fn mark_step_compensated(&self, run_id: &str, step_name: &str) -> Result<()>;
    fn finish_run(&self, run_id: &str) -> Result<()>;
    #[allow(dead_code)]
    fn load_incomplete_runs(&self) -> Result<Vec<SagaRunState>>;
}

#[derive(Debug, Clone)]
pub(super) struct FileSagaJournal {
    dir: PathBuf,
}

impl FileSagaJournal {
    pub(super) fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn default_dir() -> PathBuf {
        std::env::temp_dir().join("summer-sharding-saga")
    }

    fn path_for(&self, run_id: &str) -> PathBuf {
        self.dir.join(format!("{run_id}.json"))
    }

    fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    fn write_state(&self, state: &SagaRunState) -> Result<()> {
        self.ensure_dir()?;
        std::fs::write(
            self.path_for(state.run_id.as_str()),
            serde_json::to_vec_pretty(state)
                .map_err(|err| ShardingError::Io(std::io::Error::other(err.to_string())))?,
        )?;
        Ok(())
    }

    fn read_state(&self, run_id: &str) -> Result<SagaRunState> {
        let bytes = std::fs::read(self.path_for(run_id))?;
        serde_json::from_slice(&bytes)
            .map_err(|err| ShardingError::Io(std::io::Error::other(err.to_string())))
    }

    fn update_state(&self, run_id: &str, mutate: impl FnOnce(&mut SagaRunState)) -> Result<()> {
        let mut state = self.read_state(run_id)?;
        mutate(&mut state);
        self.write_state(&state)
    }
}

impl SagaJournal for FileSagaJournal {
    fn start_run(&self, run_id: &str) -> Result<()> {
        self.write_state(&SagaRunState {
            run_id: run_id.to_string(),
            completed_steps: Vec::new(),
            compensated_steps: Vec::new(),
        })
    }

    fn mark_step_completed(&self, run_id: &str, step_name: &str) -> Result<()> {
        self.update_state(run_id, |state| {
            if !state.completed_steps.iter().any(|step| step == step_name) {
                state.completed_steps.push(step_name.to_string());
            }
        })
    }

    fn mark_step_compensated(&self, run_id: &str, step_name: &str) -> Result<()> {
        self.update_state(run_id, |state| {
            if !state.compensated_steps.iter().any(|step| step == step_name) {
                state.compensated_steps.push(step_name.to_string());
            }
        })
    }

    fn finish_run(&self, run_id: &str) -> Result<()> {
        let path = self.path_for(run_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    fn load_incomplete_runs(&self) -> Result<Vec<SagaRunState>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut states = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                let bytes = std::fs::read(entry.path())?;
                states
                    .push(serde_json::from_slice(&bytes).map_err(|err| {
                        ShardingError::Io(std::io::Error::other(err.to_string()))
                    })?);
            }
        }
        Ok(states)
    }
}

pub struct SagaCoordinator {
    steps: Vec<Arc<dyn SagaStep>>,
    run_id: String,
    journal: Arc<dyn SagaJournal>,
}

impl SagaCoordinator {
    pub fn new(steps: Vec<Arc<dyn SagaStep>>) -> Self {
        Self {
            steps,
            run_id: format!("saga-{}-{}", std::process::id(), random::<u64>()),
            journal: Arc::new(FileSagaJournal::new(FileSagaJournal::default_dir())),
        }
    }

    pub async fn execute(&self, ctx: &dyn SagaContext) -> Result<()> {
        self.journal.start_run(self.run_id.as_str())?;
        let mut completed: Vec<Arc<dyn SagaStep>> = Vec::new();
        for step in &self.steps {
            if let Err(err) = step.execute(ctx).await {
                for executed in completed.iter().rev() {
                    if executed.compensate(ctx).await.is_ok() {
                        self.journal
                            .mark_step_compensated(self.run_id.as_str(), executed.name())?;
                    }
                }
                self.journal.finish_run(self.run_id.as_str())?;
                return Err(err);
            }
            self.journal
                .mark_step_completed(self.run_id.as_str(), step.name())?;
            completed.push(step.clone());
        }
        self.journal.finish_run(self.run_id.as_str())?;
        Ok(())
    }
}

#[cfg(test)]
pub(super) struct SagaRecoveryWorker {
    steps: Vec<Arc<dyn SagaStep>>,
    journal: Arc<dyn SagaJournal>,
}

#[cfg(test)]
impl SagaRecoveryWorker {
    pub(super) fn new(steps: Vec<Arc<dyn SagaStep>>, journal: Arc<dyn SagaJournal>) -> Self {
        Self { steps, journal }
    }

    pub async fn recover_all(&self, ctx: &dyn SagaContext) -> Result<usize> {
        let states = self.journal.load_incomplete_runs()?;
        for state in &states {
            let compensated = state
                .compensated_steps
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            for step_name in state.completed_steps.iter().rev() {
                if compensated.contains(step_name) {
                    continue;
                }
                let step = self
                    .steps
                    .iter()
                    .find(|step| step.name() == step_name)
                    .ok_or_else(|| {
                        ShardingError::Route(format!(
                            "saga recovery cannot find step `{step_name}` for run `{}`",
                            state.run_id
                        ))
                    })?;
                step.compensate(ctx).await?;
                self.journal
                    .mark_step_compensated(state.run_id.as_str(), step_name)?;
            }
            self.journal.finish_run(state.run_id.as_str())?;
        }
        Ok(states.len())
    }
}
