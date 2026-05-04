//! Unique 去重 —— 防止重复触发。
//!
//! 用户在 `sys_job.unique_key` 配置去重维度（开关）：
//! - NULL → 完全不去重（默认行为）
//! - `"params"` → 按 `params_json` 内容 hash 去重
//! - 任意其他字符串 → 字面用作 lock string（用户可填 `"global"` 实现"全局单实例运行"）
//!
//! worker 在 INSERT `sys_job_run` 之前：
//! 1. 算出实际 lock 字符串写入 `sys_job_run.unique_key`
//! 2. 查同 `(job_id, unique_key)` 是否已有 ENQUEUED/RUNNING 的 run
//! 3. 有则丢弃（state=DISCARDED, error="unique conflict"），不再执行
//!
//! 重试 / 依赖触发不参与去重检测（避免 retry 自杀），
//! 只有 Cron / Manual / Misfire / Api 触发参与。

use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::Value;
use sha2::{Digest, Sha256};
use summer_sea_orm::DbConn;

use crate::entity::{sys_job, sys_job_run};
use crate::enums::{RunState, TriggerType};

/// 应该参与去重检测的触发类型。Retry / Workflow 跳过：
/// - Retry：本身就是上一次执行的延续，参与去重会让重试永远撞死
/// - Workflow：依赖触发是上游成功才发生的，不应被自身去重锁阻塞
pub fn should_apply(trigger_type: TriggerType) -> bool {
    matches!(
        trigger_type,
        TriggerType::Cron | TriggerType::Manual | TriggerType::Misfire | TriggerType::Api
    )
}

/// 计算实际 lock 字符串。`job.unique_key` 不为 NULL 时调用。
/// 返回 None 表示用户未启用去重，应跳过本模块。
pub fn compute_lock_value(job: &sys_job::Model, params: &Value) -> Option<String> {
    let dim = job.unique_key.as_deref()?;
    let raw = match dim {
        "params" => params.to_string(),
        other => other.to_string(),
    };
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    Some(format!("{:x}", hasher.finalize()))
}

/// 检测是否冲突（已有同 lock 的 ENQUEUED / RUNNING run）。
/// 失败时（DB 错误）按 false 处理，让 worker 继续 —— 比起阻塞执行，重复执行风险更可控。
pub async fn has_conflict(db: &DbConn, job_id: i64, lock: &str) -> bool {
    match sys_job_run::Entity::find()
        .filter(sys_job_run::Column::JobId.eq(job_id))
        .filter(sys_job_run::Column::UniqueKey.eq(lock))
        .filter(sys_job_run::Column::State.is_in([RunState::Enqueued, RunState::Running]))
        .count(db)
        .await
    {
        Ok(n) => n > 0,
        Err(error) => {
            tracing::warn!(?error, job_id, "unique conflict check failed; allowing run");
            false
        }
    }
}
