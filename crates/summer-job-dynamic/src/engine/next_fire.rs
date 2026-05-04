//! 计算 job 的"下次触发时间"，前端列表 / 详情展示用。
//!
//! 各 schedule type 的算法：
//! - `Cron`：用 cron 表达式算 now 之后第一个匹配点
//! - `FixedRate`：从最近一次 scheduled_at 起 + intervalMs；若无历史，则 now + intervalMs
//! - `FixedDelay`：从最近一次 finished_at 起 + intervalMs（只有 SUCCEEDED 才算）
//! - `Oneshot`：未触发过 → fire_time；触发过 → None
//!
//! 单 job 计算函数 + 批量计算（list_jobs 用，避免 N+1 查询）。

use chrono::{Local, NaiveDateTime};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use std::collections::HashMap;
use summer_sea_orm::DbConn;

use crate::engine::misfire::parse_cron;
use crate::entity::{sys_job, sys_job_run};
use crate::enums::{RunState, ScheduleType};

/// 单 job 的"最近一次执行"信息（给前端展示）
#[derive(Debug, Clone)]
pub struct LastRunInfo {
    pub state: RunState,
    pub finished_at: Option<NaiveDateTime>,
    pub scheduled_at: NaiveDateTime,
}

/// 计算单 job 的下次触发时间。
/// `last_scheduled_at` / `last_finished_at` / `last_state` 给 FixedRate/FixedDelay 用。
pub fn compute_next_fire(
    job: &sys_job::Model,
    last_scheduled_at: Option<NaiveDateTime>,
    last_finished_at: Option<NaiveDateTime>,
    last_state: Option<RunState>,
) -> Option<NaiveDateTime> {
    if !job.enabled {
        return None;
    }
    let now = Local::now();
    match job.schedule_type {
        ScheduleType::Cron => {
            let expr = job.cron_expr.as_deref()?;
            let cron = parse_cron(expr).ok()?;
            cron.find_next_occurrence(&now, false)
                .ok()
                .map(|dt| dt.naive_local())
        }
        ScheduleType::FixedRate => {
            let interval = job.interval_ms?;
            if interval <= 0 {
                return None;
            }
            let delta = chrono::Duration::milliseconds(interval);
            // 从最近一次计划执行时间起 + interval；若无历史，从 now 起
            let base = last_scheduled_at.unwrap_or_else(|| now.naive_local());
            let mut next = base + delta;
            // 如果算出来还在过去，递推到将来（对停机后重启的场景）
            let now_naive = now.naive_local();
            while next <= now_naive {
                next += delta;
            }
            Some(next)
        }
        ScheduleType::FixedDelay => {
            let interval = job.interval_ms?;
            if interval <= 0 {
                return None;
            }
            let delta = chrono::Duration::milliseconds(interval);
            // 仅当上次成功（拿到 finished_at）才能算下次；正在跑则未知
            match (last_state, last_finished_at) {
                (
                    Some(
                        RunState::Succeeded
                        | RunState::Failed
                        | RunState::Timeout
                        | RunState::Canceled
                        | RunState::Discarded,
                    ),
                    Some(finished),
                ) => {
                    let mut next = finished + delta;
                    let now_naive = now.naive_local();
                    while next <= now_naive {
                        next += delta;
                    }
                    Some(next)
                }
                _ => Some(now.naive_local() + delta),
            }
        }
        ScheduleType::Oneshot => {
            let fire = job.fire_time?;
            // 已经触发过（last_scheduled_at 存在且 >= fire_time）就不再触发
            match last_scheduled_at {
                Some(prev) if prev >= fire => None,
                _ if fire > now.naive_local() => Some(fire),
                _ => None,
            }
        }
    }
}

/// 批量查多个 job 的"最近一次 run"（id, scheduled_at, finished_at, state）。
/// 返回 HashMap<job_id, LastRunInfo>。
pub async fn fetch_last_runs(
    db: &DbConn,
    job_ids: &[i64],
) -> Result<HashMap<i64, LastRunInfo>, sea_orm::DbErr> {
    if job_ids.is_empty() {
        return Ok(HashMap::new());
    }
    // 用 DISTINCT ON 一次拿；这里走 SeaORM 简单做法：分组查
    // 性能上 job 数量不多（< 100 是常态），N+1 没事；后续如有需要可改 raw SQL DISTINCT ON
    let mut map = HashMap::with_capacity(job_ids.len());
    for &job_id in job_ids {
        if let Some(run) = sys_job_run::Entity::find()
            .filter(sys_job_run::Column::JobId.eq(job_id))
            .order_by_desc(sys_job_run::Column::Id)
            .one(db)
            .await?
        {
            map.insert(
                job_id,
                LastRunInfo {
                    state: run.state,
                    finished_at: run.finished_at,
                    scheduled_at: run.scheduled_at,
                },
            );
        }
    }
    Ok(map)
}
