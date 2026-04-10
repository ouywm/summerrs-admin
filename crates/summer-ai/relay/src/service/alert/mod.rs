mod notifier;

use std::collections::{HashMap, HashSet};

use anyhow::Context;
use chrono::{DateTime, Days, FixedOffset, NaiveDate, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::Deserialize;
use summer::plugin::Service;
use summer_common::error::ApiResult;
use summer_sea_orm::DbConn;
use uuid::Uuid;

use self::notifier::AlertNotifierService;
use summer_ai_model::entity::alerts::alert_event::{self, AlertEventStatus};
use summer_ai_model::entity::alerts::alert_rule::{self, AlertRuleStatus};
use summer_ai_model::entity::alerts::alert_silence::{self, AlertSilenceStatus};
use summer_ai_model::entity::alerts::daily_stats;

const SHANGHAI_OFFSET_SECONDS: i32 = 8 * 3600;
const DAILY_STATS_SOURCE_DOMAIN: &str = "daily_stats";

#[derive(Clone, Service)]
pub struct DailyStatsAlertService {
    #[inject(component)]
    db: DbConn,

    #[inject(component)]
    notifier: AlertNotifierService,
}

impl DailyStatsAlertService {
    pub async fn scan_yesterday(&self) -> ApiResult<usize> {
        let today = shanghai_now().date_naive();
        let stats_date = today.checked_sub_days(Days::new(1)).unwrap_or(today);
        self.scan_day(stats_date).await
    }

    pub async fn scan_day(&self, stats_date: NaiveDate) -> ApiResult<usize> {
        let rules = alert_rule::Entity::find()
            .filter(alert_rule::Column::Status.eq(AlertRuleStatus::Enabled))
            .order_by_desc(alert_rule::Column::Severity)
            .order_by_asc(alert_rule::Column::Id)
            .all(&self.db)
            .await
            .context("查询告警规则失败")?;

        let mut touched = 0usize;
        for rule in rules {
            touched += self.scan_rule(stats_date, &rule).await?;
        }
        Ok(touched)
    }

    async fn scan_rule(&self, stats_date: NaiveDate, rule: &alert_rule::Model) -> ApiResult<usize> {
        let Some(config) = AlertRuleThresholdConfig::from_rule(rule, stats_date)? else {
            return Ok(0);
        };

        let stats_rows = self.load_daily_stats(&config).await?;
        let open_events = self.load_open_events(rule.id).await?;
        let silenced_refs = self
            .load_active_silences(rule.id, shanghai_now())
            .await?
            .into_iter()
            .map(|silence| silence.scope_key)
            .collect::<HashSet<_>>();

        let mut open_by_source = open_events
            .into_iter()
            .map(|event| (event.source_ref.clone(), event))
            .collect::<HashMap<_, _>>();

        let mut touched = 0usize;
        let mut evaluated = HashSet::new();

        for row in stats_rows {
            let source_ref = build_daily_stats_source_ref(&row);
            evaluated.insert(source_ref.clone());
            let Some(metric_value) = metric_value(&row, &config.metric_key) else {
                continue;
            };

            if config.matches(metric_value) {
                if silenced_refs.contains(&source_ref) {
                    continue;
                }

                let title = format!("{} 触发告警", rule.rule_name);
                let detail = build_alert_detail(rule, &row, metric_value, &config);
                let payload = build_alert_payload(&row, metric_value, &config);

                if let Some(existing) = open_by_source.remove(&source_ref) {
                    let mut active: alert_event::ActiveModel = existing.into();
                    active.last_triggered_at = Set(shanghai_now());
                    active.detail = Set(detail);
                    active.payload = Set(payload);
                    active.update(&self.db).await.context("更新告警事件失败")?;
                } else {
                    let created = alert_event::ActiveModel {
                        alert_rule_id: Set(rule.id),
                        event_code: Set(generate_event_code()),
                        severity: Set(rule.severity),
                        status: Set(AlertEventStatus::Open),
                        source_domain: Set(DAILY_STATS_SOURCE_DOMAIN.to_string()),
                        source_ref: Set(source_ref),
                        title: Set(title),
                        detail: Set(detail),
                        payload: Set(payload),
                        first_triggered_at: Set(shanghai_now()),
                        last_triggered_at: Set(shanghai_now()),
                        ..Default::default()
                    }
                    .insert(&self.db)
                    .await
                    .context("创建告警事件失败")?;

                    self.notify_new_event(rule, &created).await;
                }

                touched += 1;
            } else if let Some(existing) = open_by_source.remove(&source_ref) {
                let mut active: alert_event::ActiveModel = existing.into();
                active.status = Set(AlertEventStatus::Resolved);
                active.resolved_by = Set("system".to_string());
                active.resolved_time = Set(Some(shanghai_now()));
                active.update(&self.db).await.context("解决告警事件失败")?;
                touched += 1;
            }
        }

        for (source_ref, existing) in open_by_source {
            if evaluated.contains(&source_ref) {
                continue;
            }
            let mut active: alert_event::ActiveModel = existing.into();
            active.status = Set(AlertEventStatus::Resolved);
            active.resolved_by = Set("system".to_string());
            active.resolved_time = Set(Some(shanghai_now()));
            active.update(&self.db).await.context("收敛告警事件失败")?;
            touched += 1;
        }

        Ok(touched)
    }

    async fn load_daily_stats(
        &self,
        config: &AlertRuleThresholdConfig,
    ) -> ApiResult<Vec<daily_stats::Model>> {
        let mut query = daily_stats::Entity::find()
            .filter(daily_stats::Column::StatsDate.eq(config.stats_date));
        if let Some(user_id) = config.user_id {
            query = query.filter(daily_stats::Column::UserId.eq(user_id));
        }
        if let Some(project_id) = config.project_id {
            query = query.filter(daily_stats::Column::ProjectId.eq(project_id));
        }
        if let Some(channel_id) = config.channel_id {
            query = query.filter(daily_stats::Column::ChannelId.eq(channel_id));
        }
        if let Some(account_id) = config.account_id {
            query = query.filter(daily_stats::Column::AccountId.eq(account_id));
        }
        if let Some(model_name) = config.model_name.as_ref() {
            query = query.filter(daily_stats::Column::ModelName.eq(model_name.clone()));
        }

        query
            .order_by_desc(daily_stats::Column::StatsDate)
            .order_by_desc(daily_stats::Column::Id)
            .all(&self.db)
            .await
            .context("查询日度统计失败")
            .map_err(Into::into)
    }

    async fn load_open_events(&self, rule_id: i64) -> ApiResult<Vec<alert_event::Model>> {
        alert_event::Entity::find()
            .filter(alert_event::Column::AlertRuleId.eq(rule_id))
            .filter(alert_event::Column::SourceDomain.eq(DAILY_STATS_SOURCE_DOMAIN))
            .filter(
                alert_event::Column::Status
                    .is_in([AlertEventStatus::Open, AlertEventStatus::Acknowledged]),
            )
            .all(&self.db)
            .await
            .context("查询当前告警事件失败")
            .map_err(Into::into)
    }

    async fn load_active_silences(
        &self,
        rule_id: i64,
        now: DateTime<FixedOffset>,
    ) -> ApiResult<Vec<alert_silence::Model>> {
        alert_silence::Entity::find()
            .filter(alert_silence::Column::AlertRuleId.eq(rule_id))
            .filter(alert_silence::Column::Status.eq(AlertSilenceStatus::Active))
            .filter(alert_silence::Column::StartTime.lte(now))
            .filter(alert_silence::Column::EndTime.gt(now))
            .all(&self.db)
            .await
            .context("查询告警静默失败")
            .map_err(Into::into)
    }

    async fn notify_new_event(&self, rule: &alert_rule::Model, event: &alert_event::Model) {
        if let Err(error) = self.notifier.notify_new_event(rule, event).await {
            tracing::warn!(
                rule_id = rule.id,
                event_id = event.id,
                "failed to send alert notification: {error}"
            );
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertRuleThresholdConfig {
    operator: AlertComparisonOperator,
    value: f64,
    #[serde(default = "default_stats_date_offset_days")]
    stats_date_offset_days: i64,
    #[serde(default)]
    user_id: Option<i64>,
    #[serde(default)]
    project_id: Option<i64>,
    #[serde(default)]
    channel_id: Option<i64>,
    #[serde(default)]
    account_id: Option<i64>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    scope: Option<AlertScopeConfig>,
    #[serde(skip)]
    stats_date: NaiveDate,
    #[serde(skip)]
    metric_key: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AlertScopeConfig {
    user_id: Option<i64>,
    project_id: Option<i64>,
    channel_id: Option<i64>,
    account_id: Option<i64>,
    model_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AlertComparisonOperator {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

fn default_stats_date_offset_days() -> i64 {
    1
}

impl AlertRuleThresholdConfig {
    fn from_rule(rule: &alert_rule::Model, today: NaiveDate) -> ApiResult<Option<Self>> {
        if rule.metric_key.trim().is_empty() {
            return Ok(None);
        }

        let mut config: AlertRuleThresholdConfig =
            serde_json::from_value(rule.threshold_config.clone())
                .context("解析告警阈值配置失败")?;
        if let Some(scope) = config.scope.take() {
            config.user_id = config.user_id.or(scope.user_id);
            config.project_id = config.project_id.or(scope.project_id);
            config.channel_id = config.channel_id.or(scope.channel_id);
            config.account_id = config.account_id.or(scope.account_id);
            config.model_name = config.model_name.or(scope.model_name);
        }
        config.metric_key = rule.metric_key.clone();
        let offset_days = config.stats_date_offset_days.max(0) as u64;
        config.stats_date = today
            .checked_sub_days(Days::new(offset_days))
            .unwrap_or(today);
        Ok(Some(config))
    }

    fn matches(&self, metric_value: f64) -> bool {
        match self.operator {
            AlertComparisonOperator::Gt => metric_value > self.value,
            AlertComparisonOperator::Gte => metric_value >= self.value,
            AlertComparisonOperator::Lt => metric_value < self.value,
            AlertComparisonOperator::Lte => metric_value <= self.value,
            AlertComparisonOperator::Eq => (metric_value - self.value).abs() < f64::EPSILON,
        }
    }
}

fn metric_value(row: &daily_stats::Model, metric_key: &str) -> Option<f64> {
    match metric_key {
        "request_count" => Some(row.request_count as f64),
        "success_count" => Some(row.success_count as f64),
        "fail_count" => Some(row.fail_count as f64),
        "success_rate" => (row.request_count > 0)
            .then_some(row.success_count as f64 * 100.0 / row.request_count as f64),
        "fail_rate" => (row.request_count > 0)
            .then_some(row.fail_count as f64 * 100.0 / row.request_count as f64),
        "prompt_tokens" => Some(row.prompt_tokens as f64),
        "completion_tokens" => Some(row.completion_tokens as f64),
        "total_tokens" => Some(row.total_tokens as f64),
        "cached_tokens" => Some(row.cached_tokens as f64),
        "reasoning_tokens" => Some(row.reasoning_tokens as f64),
        "quota" => Some(row.quota as f64),
        "cost_total" => Some(row.cost_total.to_string().parse::<f64>().unwrap_or(0.0)),
        "avg_elapsed_time" => Some(row.avg_elapsed_time as f64),
        "avg_first_token_time" => Some(row.avg_first_token_time as f64),
        _ => None,
    }
}

fn build_daily_stats_source_ref(row: &daily_stats::Model) -> String {
    format!(
        "{}|u:{}|p:{}|c:{}|a:{}|m:{}",
        row.stats_date, row.user_id, row.project_id, row.channel_id, row.account_id, row.model_name
    )
}

fn build_alert_detail(
    rule: &alert_rule::Model,
    row: &daily_stats::Model,
    metric_value: f64,
    config: &AlertRuleThresholdConfig,
) -> String {
    format!(
        "{}: {}={} 命中 {:?} {}（statsDate={}, userId={}, projectId={}, channelId={}, accountId={}, model={})",
        rule.rule_name,
        rule.metric_key,
        metric_value,
        config.operator,
        config.value,
        row.stats_date,
        row.user_id,
        row.project_id,
        row.channel_id,
        row.account_id,
        row.model_name
    )
}

fn build_alert_payload(
    row: &daily_stats::Model,
    metric_value: f64,
    config: &AlertRuleThresholdConfig,
) -> serde_json::Value {
    serde_json::json!({
        "statsDate": row.stats_date,
        "userId": row.user_id,
        "projectId": row.project_id,
        "channelId": row.channel_id,
        "accountId": row.account_id,
        "modelName": row.model_name,
        "metricKey": config.metric_key,
        "metricValue": metric_value,
        "threshold": config.value,
        "operator": format!("{:?}", config.operator),
        "requestCount": row.request_count,
        "successCount": row.success_count,
        "failCount": row.fail_count,
        "quota": row.quota,
        "costTotal": row.cost_total.to_string(),
        "avgElapsedTime": row.avg_elapsed_time,
        "avgFirstTokenTime": row.avg_first_token_time,
    })
}

fn generate_event_code() -> String {
    format!("altevt_{}", Uuid::new_v4().simple())
}

fn shanghai_now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(
        &FixedOffset::east_opt(SHANGHAI_OFFSET_SECONDS).expect("valid shanghai offset"),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, Utc};
    use sea_orm::prelude::BigDecimal;

    use super::{
        AlertComparisonOperator, AlertRuleThresholdConfig, build_daily_stats_source_ref,
        metric_value,
    };
    use summer_ai_model::entity::alerts::daily_stats;

    fn sample_row() -> daily_stats::Model {
        daily_stats::Model {
            id: 1,
            stats_date: NaiveDate::from_ymd_opt(2026, 4, 10).expect("valid date"),
            user_id: 11,
            project_id: 22,
            channel_id: 33,
            account_id: 44,
            model_name: "gpt-5.4".into(),
            request_count: 20,
            success_count: 18,
            fail_count: 2,
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
            cached_tokens: 100,
            reasoning_tokens: 50,
            quota: 3000,
            cost_total: BigDecimal::from(12),
            avg_elapsed_time: 800,
            avg_first_token_time: 120,
            create_time: Utc::now().fixed_offset(),
        }
    }

    #[test]
    fn metric_value_supports_success_rate_percentage() {
        let row = sample_row();
        let value = metric_value(&row, "success_rate").expect("success rate");
        assert!((value - 90.0).abs() < 0.0001);
    }

    #[test]
    fn build_daily_stats_source_ref_is_stable() {
        let row = sample_row();
        assert_eq!(
            build_daily_stats_source_ref(&row),
            "2026-04-10|u:11|p:22|c:33|a:44|m:gpt-5.4"
        );
    }

    #[test]
    fn alert_threshold_config_matches_comparison() {
        let config = AlertRuleThresholdConfig {
            operator: AlertComparisonOperator::Gte,
            value: 95.0,
            stats_date_offset_days: 1,
            user_id: None,
            project_id: None,
            channel_id: None,
            account_id: None,
            model_name: None,
            scope: None,
            stats_date: NaiveDate::from_ymd_opt(2026, 4, 10).expect("valid date"),
            metric_key: "success_rate".into(),
        };

        assert!(!config.matches(90.0));
        assert!(config.matches(95.0));
    }
}
