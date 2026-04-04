use chrono::{DateTime, FixedOffset, NaiveDate};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::alert_event::{self, AlertEventStatus};
use crate::entity::alert_rule::{self, AlertRuleStatus, AlertSeverity};
use crate::entity::alert_silence::{self, SilenceStatus};
use crate::entity::daily_stats;

/// 告警规则 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertRuleVo {
    pub id: i64,
    pub domain_code: String,
    pub rule_code: String,
    pub rule_name: String,
    pub severity: AlertSeverity,
    pub metric_key: String,
    pub condition_expr: String,
    pub threshold_config: serde_json::Value,
    pub channel_config: serde_json::Value,
    pub silence_seconds: i32,
    pub status: AlertRuleStatus,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl AlertRuleVo {
    pub fn from_model(m: alert_rule::Model) -> Self {
        Self {
            id: m.id,
            domain_code: m.domain_code,
            rule_code: m.rule_code,
            rule_name: m.rule_name,
            severity: m.severity,
            metric_key: m.metric_key,
            condition_expr: m.condition_expr,
            threshold_config: m.threshold_config,
            channel_config: m.channel_config,
            silence_seconds: m.silence_seconds,
            status: m.status,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

/// 告警事件 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertEventVo {
    pub id: i64,
    pub alert_rule_id: i64,
    pub event_code: String,
    pub severity: AlertSeverity,
    pub status: AlertEventStatus,
    pub source_domain: String,
    pub source_ref: String,
    pub title: String,
    pub detail: String,
    pub payload: serde_json::Value,
    pub first_triggered_at: DateTime<FixedOffset>,
    pub last_triggered_at: DateTime<FixedOffset>,
    pub ack_by: String,
    pub ack_time: Option<DateTime<FixedOffset>>,
    pub resolved_by: String,
    pub resolved_time: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}

impl AlertEventVo {
    pub fn from_model(m: alert_event::Model) -> Self {
        Self {
            id: m.id,
            alert_rule_id: m.alert_rule_id,
            event_code: m.event_code,
            severity: m.severity,
            status: m.status,
            source_domain: m.source_domain,
            source_ref: m.source_ref,
            title: m.title,
            detail: m.detail,
            payload: m.payload,
            first_triggered_at: m.first_triggered_at,
            last_triggered_at: m.last_triggered_at,
            ack_by: m.ack_by,
            ack_time: m.ack_time,
            resolved_by: m.resolved_by,
            resolved_time: m.resolved_time,
            create_time: m.create_time,
        }
    }
}

/// 告警静默 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertSilenceVo {
    pub id: i64,
    pub alert_rule_id: i64,
    pub scope_type: String,
    pub scope_key: String,
    pub reason: String,
    pub status: SilenceStatus,
    pub create_by: String,
    pub start_time: DateTime<FixedOffset>,
    pub end_time: DateTime<FixedOffset>,
    pub create_time: DateTime<FixedOffset>,
}

impl AlertSilenceVo {
    pub fn from_model(m: alert_silence::Model) -> Self {
        Self {
            id: m.id,
            alert_rule_id: m.alert_rule_id,
            scope_type: m.scope_type,
            scope_key: m.scope_key,
            reason: m.reason,
            status: m.status,
            create_by: m.create_by,
            start_time: m.start_time,
            end_time: m.end_time,
            create_time: m.create_time,
        }
    }
}

/// 日度统计 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatsVo {
    pub id: i64,
    pub stats_date: NaiveDate,
    pub user_id: i64,
    pub project_id: i64,
    pub channel_id: i64,
    pub account_id: i64,
    pub model_name: String,
    pub request_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub reasoning_tokens: i64,
    pub quota: i64,
    pub cost_total: String,
    pub avg_elapsed_time: i32,
    pub avg_first_token_time: i32,
    pub create_time: DateTime<FixedOffset>,
}

impl DailyStatsVo {
    pub fn from_model(m: daily_stats::Model) -> Self {
        Self {
            id: m.id,
            stats_date: m.stats_date,
            user_id: m.user_id,
            project_id: m.project_id,
            channel_id: m.channel_id,
            account_id: m.account_id,
            model_name: m.model_name,
            request_count: m.request_count,
            success_count: m.success_count,
            fail_count: m.fail_count,
            prompt_tokens: m.prompt_tokens,
            completion_tokens: m.completion_tokens,
            total_tokens: m.total_tokens,
            cached_tokens: m.cached_tokens,
            reasoning_tokens: m.reasoning_tokens,
            quota: m.quota,
            cost_total: m.cost_total.to_string(),
            avg_elapsed_time: m.avg_elapsed_time,
            avg_first_token_time: m.avg_first_token_time,
            create_time: m.create_time,
        }
    }
}
