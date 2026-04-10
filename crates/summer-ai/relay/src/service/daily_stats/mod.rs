use anyhow::Context;
use chrono::{DateTime, Days, FixedOffset, NaiveDate, Utc};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use summer::plugin::Service;
use summer_common::error::ApiResult;
use summer_sea_orm::DbConn;

const SHANGHAI_OFFSET_SECONDS: i32 = 8 * 3600;

#[derive(Clone, Service)]
pub struct DailyStatsService {
    #[inject(component)]
    db: DbConn,
}

impl DailyStatsService {
    pub async fn aggregate_day(&self, stats_date: NaiveDate) -> ApiResult<u64> {
        let statement = build_aggregate_statement(stats_date);

        let result = self
            .db
            .execute_raw(statement)
            .await
            .context("聚合 daily_stats 失败")?;

        Ok(result.rows_affected())
    }

    pub async fn aggregate_yesterday(&self) -> ApiResult<u64> {
        let today = shanghai_now().date_naive();
        let stats_date = today.checked_sub_days(Days::new(1)).unwrap_or(today);
        self.aggregate_day(stats_date).await
    }
}

fn build_aggregate_statement(stats_date: NaiveDate) -> Statement {
    let (start_at, end_at) = stats_day_bounds(stats_date);
    let sql = r#"
        INSERT INTO ai.daily_stats (
            stats_date,
            user_id,
            project_id,
            channel_id,
            account_id,
            model_name,
            request_count,
            success_count,
            fail_count,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens,
            reasoning_tokens,
            quota,
            cost_total,
            avg_elapsed_time,
            avg_first_token_time
        )
        SELECT
            $1::date AS stats_date,
            COALESCE(l.user_id, 0) AS user_id,
            COALESCE(l.project_id, 0) AS project_id,
            COALESCE(l.channel_id, 0) AS channel_id,
            COALESCE(l.account_id, 0) AS account_id,
            COALESCE(l.model_name, '') AS model_name,
            COUNT(*)::bigint AS request_count,
            COUNT(*) FILTER (WHERE l.status = 1)::bigint AS success_count,
            COUNT(*) FILTER (WHERE l.status = 2)::bigint AS fail_count,
            COALESCE(SUM(l.prompt_tokens), 0)::bigint AS prompt_tokens,
            COALESCE(SUM(l.completion_tokens), 0)::bigint AS completion_tokens,
            COALESCE(SUM(l.total_tokens), 0)::bigint AS total_tokens,
            COALESCE(SUM(l.cached_tokens), 0)::bigint AS cached_tokens,
            COALESCE(SUM(l.reasoning_tokens), 0)::bigint AS reasoning_tokens,
            COALESCE(SUM(l.quota), 0)::bigint AS quota,
            COALESCE(SUM(l.cost_total), 0)::decimal(20,10) AS cost_total,
            COALESCE(ROUND(AVG(l.elapsed_time)), 0)::int AS avg_elapsed_time,
            COALESCE(ROUND(AVG(NULLIF(l.first_token_time, 0))), 0)::int AS avg_first_token_time
        FROM ai.log AS l
        WHERE l.log_type = 2
          AND l.create_time >= $2
          AND l.create_time < $3
        GROUP BY GROUPING SETS (
            (l.user_id, l.project_id, l.channel_id, l.account_id, l.model_name),
            ()
        )
        ON CONFLICT (stats_date, user_id, project_id, channel_id, account_id, model_name)
        DO UPDATE SET
            request_count = EXCLUDED.request_count,
            success_count = EXCLUDED.success_count,
            fail_count = EXCLUDED.fail_count,
            prompt_tokens = EXCLUDED.prompt_tokens,
            completion_tokens = EXCLUDED.completion_tokens,
            total_tokens = EXCLUDED.total_tokens,
            cached_tokens = EXCLUDED.cached_tokens,
            reasoning_tokens = EXCLUDED.reasoning_tokens,
            quota = EXCLUDED.quota,
            cost_total = EXCLUDED.cost_total,
            avg_elapsed_time = EXCLUDED.avg_elapsed_time,
            avg_first_token_time = EXCLUDED.avg_first_token_time
        "#;

    Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        [stats_date.into(), start_at.into(), end_at.into()],
    )
}

fn shanghai_now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(
        &FixedOffset::east_opt(SHANGHAI_OFFSET_SECONDS).expect("valid shanghai offset"),
    )
}

fn stats_day_bounds(stats_date: NaiveDate) -> (DateTime<FixedOffset>, DateTime<FixedOffset>) {
    let offset = FixedOffset::east_opt(SHANGHAI_OFFSET_SECONDS).expect("valid shanghai offset");
    let start_at = stats_date
        .and_hms_opt(0, 0, 0)
        .expect("valid start of day")
        .and_local_timezone(offset)
        .single()
        .expect("single timezone conversion");
    let end_at = (stats_date + Days::new(1))
        .and_hms_opt(0, 0, 0)
        .expect("valid next start of day")
        .and_local_timezone(offset)
        .single()
        .expect("single timezone conversion");
    (start_at, end_at)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{build_aggregate_statement, stats_day_bounds};

    #[test]
    fn aggregate_statement_writes_upsert_sql_against_ai_log() {
        let stats_date = NaiveDate::from_ymd_opt(2026, 4, 10).expect("valid date");
        let statement = build_aggregate_statement(stats_date);
        let sql = &statement.sql;

        assert!(sql.contains("INSERT INTO ai.daily_stats"));
        assert!(sql.contains("FROM ai.log AS l"));
        assert!(sql.contains("ON CONFLICT"));
        assert!(sql.contains("l.create_time >= $2"));
        assert!(sql.contains("l.create_time < $3"));
        assert!(sql.contains("l.log_type = 2"));
        assert!(sql.contains("COUNT(*) FILTER (WHERE l.status = 1)"));
    }

    #[test]
    fn aggregate_statement_includes_global_summary_grouping() {
        let stats_date = NaiveDate::from_ymd_opt(2026, 4, 10).expect("valid date");
        let statement = build_aggregate_statement(stats_date);
        let sql = &statement.sql;

        assert!(sql.contains("GROUPING SETS"));
        assert!(sql.contains("COALESCE(l.user_id, 0)"));
        assert!(sql.contains("COALESCE(l.project_id, 0)"));
        assert!(sql.contains("COALESCE(l.channel_id, 0)"));
        assert!(sql.contains("COALESCE(l.account_id, 0)"));
        assert!(sql.contains("COALESCE(l.model_name, '')"));
    }

    #[test]
    fn stats_day_bounds_use_shanghai_natural_day() {
        let stats_date = NaiveDate::from_ymd_opt(2026, 4, 10).expect("valid date");
        let (start_at, end_at) = stats_day_bounds(stats_date);

        assert_eq!(start_at.to_rfc3339(), "2026-04-10T00:00:00+08:00");
        assert_eq!(end_at.to_rfc3339(), "2026-04-11T00:00:00+08:00");
    }
}
