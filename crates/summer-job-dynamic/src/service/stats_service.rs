//! 执行统计聚合 —— 给前端仪表盘用，DB 层 GROUP BY 算好返回，避免前端拉原始数据自己算。
//!
//! 两个接口：
//! - 全局概览（首页用）：总任务数 / 启用数 / 时间窗内触发数 / 成功率 / 当前在跑 / 失败 top
//! - 单任务统计（详情页用）：成功率 / 平均耗时 / P50 / P99 / 趋势点序列
//!
//! period 解析 + 时间桶粒度：
//! - `1h`  → 1 小时窗口，`5min` 桶（12 个点）
//! - `24h` → 24 小时窗口，`1hour` 桶（24 个点）
//! - `7d`  → 7 天窗口，`1day` 桶（7 个点）
//! - `30d` → 30 天窗口，`1day` 桶（30 个点）

use anyhow::Context;
use chrono::NaiveDateTime;
use schemars::JsonSchema;
use sea_orm::{DatabaseBackend, FromQueryResult, Statement, Value as SeaValue};
use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

#[derive(Clone, Service)]
pub struct StatsService {
    #[inject(component)]
    db: DbConn,
}

// ---------------------------------------------------------------------------
// 时间窗口
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StatsPeriod {
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "24h")]
    #[default]
    OneDay,
    #[serde(rename = "7d")]
    SevenDays,
    #[serde(rename = "30d")]
    ThirtyDays,
}

impl StatsPeriod {
    fn pg_interval(self) -> &'static str {
        match self {
            Self::OneHour => "1 hour",
            Self::OneDay => "1 day",
            Self::SevenDays => "7 days",
            Self::ThirtyDays => "30 days",
        }
    }
    fn bucket(self) -> &'static str {
        match self {
            Self::OneHour => "5 minutes",
            Self::OneDay => "1 hour",
            Self::SevenDays => "1 day",
            Self::ThirtyDays => "1 day",
        }
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsQuery {
    #[serde(default)]
    pub period: StatsPeriod,
}

// ---------------------------------------------------------------------------
// VO
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsOverviewVo {
    pub total_jobs: i64,
    pub enabled_jobs: i64,
    pub triggered_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    /// 0.0 - 1.0；触发数为 0 时返回 null
    pub success_rate: Option<f64>,
    pub in_flight_count: i64,
    pub top_failed_jobs: Vec<TopFailedJob>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopFailedJob {
    pub job_id: i64,
    pub name: String,
    pub fail_count: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobStatsVo {
    pub job_id: i64,
    pub triggered_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub success_rate: Option<f64>,
    /// 平均耗时毫秒（仅 SUCCEEDED 计入）
    pub avg_duration_ms: Option<f64>,
    pub p50: Option<f64>,
    pub p99: Option<f64>,
    pub points: Vec<StatsPoint>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsPoint {
    /// 桶起始时间（NaiveDateTime，本地时区）
    pub ts: NaiveDateTime,
    pub total: i64,
    pub succeeded: i64,
    pub failed: i64,
}

// ---------------------------------------------------------------------------
// FromQueryResult 内部行
// ---------------------------------------------------------------------------

#[derive(Debug, FromQueryResult)]
struct CountRow {
    total: i64,
    enabled: i64,
}

#[derive(Debug, FromQueryResult)]
struct RunCountRow {
    total: i64,
    ok: i64,
    fail: i64,
}

#[derive(Debug, FromQueryResult)]
struct InFlightRow {
    cnt: i64,
}

#[derive(Debug, FromQueryResult)]
struct TopFailedRow {
    job_id: i64,
    name: String,
    cnt: i64,
}

#[derive(Debug, FromQueryResult)]
struct DurationRow {
    avg_ms: Option<f64>,
    p50: Option<f64>,
    p99: Option<f64>,
}

#[derive(Debug, FromQueryResult)]
struct PointRow {
    ts: NaiveDateTime,
    total: i64,
    ok: i64,
    fail: i64,
}

// ---------------------------------------------------------------------------
// Service 实现
// ---------------------------------------------------------------------------

impl StatsService {
    pub async fn overview(&self, period: StatsPeriod) -> ApiResult<StatsOverviewVo> {
        let interval = period.pg_interval();

        let job_counts = CountRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r#"SELECT count(*)::bigint AS total,
                      count(*) FILTER (WHERE enabled)::bigint AS enabled
               FROM sys.job"#,
            [],
        ))
        .one(&self.db)
        .await
        .context("查询任务总数失败")?
        .ok_or_else(|| ApiErrors::Internal(anyhow::anyhow!("count query returned no row")))?;

        let run_counts = RunCountRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                r#"SELECT count(*)::bigint AS total,
                          count(*) FILTER (WHERE state = 'SUCCEEDED')::bigint AS ok,
                          count(*) FILTER (WHERE state = 'FAILED')::bigint AS fail
                   FROM sys.job_run
                   WHERE scheduled_at >= NOW() - INTERVAL '{interval}'"#
            ),
            [],
        ))
        .one(&self.db)
        .await
        .context("查询触发统计失败")?
        .unwrap_or(RunCountRow {
            total: 0,
            ok: 0,
            fail: 0,
        });

        let in_flight = InFlightRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r#"SELECT count(*)::bigint AS cnt
               FROM sys.job_run
               WHERE state IN ('ENQUEUED', 'RUNNING')"#,
            [],
        ))
        .one(&self.db)
        .await
        .context("查询在跑数失败")?
        .map(|r| r.cnt)
        .unwrap_or(0);

        let top_failed = TopFailedRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                r#"SELECT r.job_id::bigint AS job_id, j.name AS name, count(*)::bigint AS cnt
                   FROM sys.job_run r JOIN sys.job j ON j.id = r.job_id
                   WHERE r.state = 'FAILED'
                     AND r.scheduled_at >= NOW() - INTERVAL '{interval}'
                   GROUP BY r.job_id, j.name
                   ORDER BY cnt DESC
                   LIMIT 5"#
            ),
            [],
        ))
        .all(&self.db)
        .await
        .context("查询失败 top 任务失败")?;

        Ok(StatsOverviewVo {
            total_jobs: job_counts.total,
            enabled_jobs: job_counts.enabled,
            triggered_count: run_counts.total,
            succeeded_count: run_counts.ok,
            failed_count: run_counts.fail,
            success_rate: success_rate(run_counts.total, run_counts.ok),
            in_flight_count: in_flight,
            top_failed_jobs: top_failed
                .into_iter()
                .map(|r| TopFailedJob {
                    job_id: r.job_id,
                    name: r.name,
                    fail_count: r.cnt,
                })
                .collect(),
        })
    }

    pub async fn job_stats(&self, job_id: i64, period: StatsPeriod) -> ApiResult<JobStatsVo> {
        let interval = period.pg_interval();
        let bucket = period.bucket();

        let counts = RunCountRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                r#"SELECT count(*)::bigint AS total,
                          count(*) FILTER (WHERE state = 'SUCCEEDED')::bigint AS ok,
                          count(*) FILTER (WHERE state = 'FAILED')::bigint AS fail
                   FROM sys.job_run
                   WHERE job_id = $1 AND scheduled_at >= NOW() - INTERVAL '{interval}'"#
            ),
            [SeaValue::BigInt(Some(job_id))],
        ))
        .one(&self.db)
        .await
        .context("查询任务统计失败")?
        .unwrap_or(RunCountRow {
            total: 0,
            ok: 0,
            fail: 0,
        });

        let duration = DurationRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                r#"SELECT
                       AVG(EXTRACT(EPOCH FROM (finished_at - started_at)) * 1000)::float8 AS avg_ms,
                       PERCENTILE_CONT(0.5) WITHIN GROUP (
                           ORDER BY EXTRACT(EPOCH FROM (finished_at - started_at)) * 1000
                       )::float8 AS p50,
                       PERCENTILE_CONT(0.99) WITHIN GROUP (
                           ORDER BY EXTRACT(EPOCH FROM (finished_at - started_at)) * 1000
                       )::float8 AS p99
                   FROM sys.job_run
                   WHERE job_id = $1
                     AND scheduled_at >= NOW() - INTERVAL '{interval}'
                     AND state = 'SUCCEEDED'
                     AND started_at IS NOT NULL AND finished_at IS NOT NULL"#
            ),
            [SeaValue::BigInt(Some(job_id))],
        ))
        .one(&self.db)
        .await
        .context("查询耗时分布失败")?
        .unwrap_or(DurationRow {
            avg_ms: None,
            p50: None,
            p99: None,
        });

        let points = PointRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                r#"SELECT
                       date_bin(INTERVAL '{bucket}', scheduled_at, TIMESTAMP '2000-01-01')::timestamp AS ts,
                       count(*)::bigint AS total,
                       count(*) FILTER (WHERE state = 'SUCCEEDED')::bigint AS ok,
                       count(*) FILTER (WHERE state = 'FAILED')::bigint AS fail
                   FROM sys.job_run
                   WHERE job_id = $1 AND scheduled_at >= NOW() - INTERVAL '{interval}'
                   GROUP BY 1
                   ORDER BY 1"#
            ),
            [SeaValue::BigInt(Some(job_id))],
        ))
        .all(&self.db)
        .await
        .context("查询趋势点失败")?;

        Ok(JobStatsVo {
            job_id,
            triggered_count: counts.total,
            succeeded_count: counts.ok,
            failed_count: counts.fail,
            success_rate: success_rate(counts.total, counts.ok),
            avg_duration_ms: duration.avg_ms,
            p50: duration.p50,
            p99: duration.p99,
            points: points
                .into_iter()
                .map(|r| StatsPoint {
                    ts: r.ts,
                    total: r.total,
                    succeeded: r.ok,
                    failed: r.fail,
                })
                .collect(),
        })
    }
}

fn success_rate(total: i64, ok: i64) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some(ok as f64 / total as f64)
    }
}
