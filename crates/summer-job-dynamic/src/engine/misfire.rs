//! Misfire 策略 —— 调度器停机/主切换错过 cron 触发点后的补偿语义。
//!
//! ## 何时检测
//! - 启动期加载 enabled jobs 时
//! - 启用 / 更新 cron 任务后
//!
//! ## 检测逻辑
//! 1. 找最近一次 SUCCEEDED 的 finished_at 作为基线；没有 run 记录时用 `update_time`
//! 2. 用 cron 表达式算"现在之前最近一次应触发的时间"`previous`
//! 3. 如果 `previous > 基线` → 错过了；按 `model.misfire` 处理：
//!    - `FireNow`：立即补跑一次（trigger_type=Misfire）
//!    - `Ignore` / `Reschedule`：跳过，等下一次 cron tick
//!
//! ## 边界
//! - 仅 `ScheduleType::Cron` 任务参与（OneShot 走 fire_time 自己判断；FixedRate 没有
//!   "应该触发的时间点"概念）
//! - 错过 N 次也只补一次（防风暴；Sidekiq / xxl-job 都是这个语义）
//! - 时区按 [`chrono::Local`]，与 worker 写入 `scheduled_at` 一致

use anyhow::Context;
use chrono::{Local, NaiveDateTime, TimeZone};
use croner::Cron;
use croner::parser::{CronParser, Seconds};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer_sea_orm::DbConn;

use crate::entity::{sys_job, sys_job_run};
use crate::enums::{MisfireStrategy, RunState, ScheduleType};

/// 一次 misfire 评估结果。
#[derive(Debug, Clone)]
pub struct MisfireDecision {
    /// 是否需要立即补跑（仅 FireNow + missed > 0 才为 true）
    pub should_fire: bool,
    /// 估算错过的次数（向前迭代统计，<=100 防御性截断）
    pub missed_count: u32,
    /// 用于比对的"基线时间"：最近 SUCCEEDED 的 finished_at 或 model.update_time
    pub baseline: NaiveDateTime,
    /// 当前时间之前最近一次按 cron 应触发的时刻
    pub previous_scheduled: Option<NaiveDateTime>,
}

impl MisfireDecision {
    pub fn no_op(reason: &'static str, baseline: NaiveDateTime) -> Self {
        tracing::trace!(reason, "misfire: no-op");
        Self {
            should_fire: false,
            missed_count: 0,
            baseline,
            previous_scheduled: None,
        }
    }
}

/// 评估单个任务是否需要 misfire 补跑。
pub async fn evaluate(db: &DbConn, job: &sys_job::Model) -> anyhow::Result<MisfireDecision> {
    let baseline = load_baseline(db, job).await?;

    if !matches!(job.schedule_type, ScheduleType::Cron) {
        return Ok(MisfireDecision::no_op("not cron job", baseline));
    }
    let Some(cron_expr) = job.cron_expr.as_deref() else {
        return Ok(MisfireDecision::no_op("cron_expr empty", baseline));
    };

    let cron =
        parse_cron(cron_expr).with_context(|| format!("解析 cron 表达式失败: {}", cron_expr))?;

    let now_local = Local::now();
    let previous = match cron.find_previous_occurrence(&now_local, false) {
        Ok(t) => t.naive_local(),
        Err(error) => {
            tracing::warn!(?error, cron = cron_expr, "find_previous_occurrence failed");
            return Ok(MisfireDecision::no_op("no previous occurrence", baseline));
        }
    };

    if previous <= baseline {
        // 基线之后还没有应触发点（或上次正好赶上了）
        return Ok(MisfireDecision {
            should_fire: false,
            missed_count: 0,
            baseline,
            previous_scheduled: Some(previous),
        });
    }

    let missed_count = count_missed(&cron, baseline, now_local);
    let should_fire = matches!(job.misfire, MisfireStrategy::FireNow);

    Ok(MisfireDecision {
        should_fire,
        missed_count,
        baseline,
        previous_scheduled: Some(previous),
    })
}

/// 估算从 baseline 到 now 之间 cron 应该触发的次数（最多 100，防御性截断）。
fn count_missed(cron: &Cron, baseline: NaiveDateTime, now: chrono::DateTime<Local>) -> u32 {
    let mut cursor = match Local.from_local_datetime(&baseline).single() {
        Some(t) => t,
        None => return 1, // baseline 无法转 Local（DST 模糊），保守报 1 次
    };
    let mut count = 0u32;
    while count < 100 {
        match cron.find_next_occurrence(&cursor, false) {
            Ok(next) if next <= now => {
                count += 1;
                cursor = next;
            }
            _ => break,
        }
    }
    count
}

async fn load_baseline(db: &DbConn, job: &sys_job::Model) -> anyhow::Result<NaiveDateTime> {
    let last = sys_job_run::Entity::find()
        .filter(sys_job_run::Column::JobId.eq(job.id))
        .filter(sys_job_run::Column::State.eq(RunState::Succeeded))
        .order_by_desc(sys_job_run::Column::FinishedAt)
        .one(db)
        .await
        .context("查询最近 SUCCEEDED 失败")?;
    Ok(last.and_then(|r| r.finished_at).unwrap_or(job.update_time))
}

pub fn parse_cron(expr: &str) -> Result<Cron, croner::errors::CronError> {
    // tokio-cron-scheduler 同款配置：6 字段（秒分时日月周）必填
    CronParser::builder()
        .seconds(Seconds::Required)
        .build()
        .parse(expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as CDur;

    #[test]
    fn parse_six_field_cron() {
        let c = parse_cron("0 0 * * * *").expect("hourly cron should parse");
        let now = Local::now();
        let prev = c
            .find_previous_occurrence(&now, false)
            .expect("must have prev");
        assert!(prev <= now);
    }

    #[test]
    fn count_missed_within_window() {
        let cron = parse_cron("0 * * * * *").unwrap(); // 每分钟
        let now = Local::now();
        let baseline = (now - CDur::minutes(5)).naive_local();
        let missed = count_missed(&cron, baseline, now);
        // 5 分钟窗口至少错过 4 次（边界视当前秒情况）
        assert!((4..=6).contains(&missed), "missed = {missed}");
    }

    #[test]
    fn count_missed_zero_when_recent() {
        let cron = parse_cron("0 * * * * *").unwrap();
        let now = Local::now();
        let baseline = now.naive_local();
        let missed = count_missed(&cron, baseline, now);
        assert_eq!(missed, 0);
    }
}
