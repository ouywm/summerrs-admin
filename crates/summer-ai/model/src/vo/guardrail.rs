use chrono::{DateTime, FixedOffset, NaiveDate};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::guardrail_config;
use crate::entity::guardrail_metric_daily;
use crate::entity::guardrail_rule::{self, GuardrailSeverity};
use crate::entity::guardrail_violation;
use crate::entity::prompt_protection_rule::{self, PromptProtectionStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailConfigVo {
    pub id: i64,
    pub scope_type: String,
    pub organization_id: i64,
    pub project_id: i64,
    pub enabled: bool,
    pub mode: String,
    pub system_rules: serde_json::Value,
    pub allowed_file_types: serde_json::Value,
    pub max_file_size_mb: i32,
    pub pii_action: String,
    pub secret_action: String,
    pub metadata: serde_json::Value,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl GuardrailConfigVo {
    pub fn from_model(m: guardrail_config::Model) -> Self {
        Self {
            id: m.id,
            scope_type: m.scope_type,
            organization_id: m.organization_id,
            project_id: m.project_id,
            enabled: m.enabled,
            mode: m.mode,
            system_rules: m.system_rules,
            allowed_file_types: m.allowed_file_types,
            max_file_size_mb: m.max_file_size_mb,
            pii_action: m.pii_action,
            secret_action: m.secret_action,
            metadata: m.metadata,
            remark: m.remark,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailRuleVo {
    pub id: i64,
    pub guardrail_config_id: i64,
    pub rule_code: String,
    pub rule_name: String,
    pub rule_type: String,
    pub phase: String,
    pub action: String,
    pub priority: i32,
    pub enabled: bool,
    pub severity: GuardrailSeverity,
    pub model_pattern: String,
    pub endpoint_pattern: String,
    pub condition_json: serde_json::Value,
    pub rule_config: serde_json::Value,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl GuardrailRuleVo {
    pub fn from_model(m: guardrail_rule::Model) -> Self {
        Self {
            id: m.id,
            guardrail_config_id: m.guardrail_config_id,
            rule_code: m.rule_code,
            rule_name: m.rule_name,
            rule_type: m.rule_type,
            phase: m.phase,
            action: m.action,
            priority: m.priority,
            enabled: m.enabled,
            severity: m.severity,
            model_pattern: m.model_pattern,
            endpoint_pattern: m.endpoint_pattern,
            condition_json: m.condition_json,
            rule_config: m.rule_config,
            remark: m.remark,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailViolationVo {
    pub id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub user_id: i64,
    pub token_id: i64,
    pub rule_id: i64,
    pub request_id: String,
    pub phase: String,
    pub category: String,
    pub action_taken: String,
    pub model_name: String,
    pub endpoint: String,
    pub matched_pattern: String,
    pub sample_excerpt: String,
    pub severity: i16,
    pub latency_ms: i32,
    pub create_time: DateTime<FixedOffset>,
}

impl GuardrailViolationVo {
    pub fn from_model(m: guardrail_violation::Model) -> Self {
        Self {
            id: m.id,
            organization_id: m.organization_id,
            project_id: m.project_id,
            user_id: m.user_id,
            token_id: m.token_id,
            rule_id: m.rule_id,
            request_id: m.request_id,
            phase: m.phase,
            category: m.category,
            action_taken: m.action_taken,
            model_name: m.model_name,
            endpoint: m.endpoint,
            matched_pattern: m.matched_pattern,
            sample_excerpt: m.sample_excerpt,
            severity: m.severity,
            latency_ms: m.latency_ms,
            create_time: m.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailMetricDailyVo {
    pub id: i64,
    pub stats_date: NaiveDate,
    pub organization_id: i64,
    pub project_id: i64,
    pub rule_id: i64,
    pub rule_code: String,
    pub requests_evaluated: i64,
    pub passed_count: i64,
    pub blocked_count: i64,
    pub redacted_count: i64,
    pub warned_count: i64,
    pub flagged_count: i64,
    pub avg_latency_ms: i32,
    pub create_time: DateTime<FixedOffset>,
}

impl GuardrailMetricDailyVo {
    pub fn from_model(m: guardrail_metric_daily::Model) -> Self {
        Self {
            id: m.id,
            stats_date: m.stats_date,
            organization_id: m.organization_id,
            project_id: m.project_id,
            rule_id: m.rule_id,
            rule_code: m.rule_code,
            requests_evaluated: m.requests_evaluated,
            passed_count: m.passed_count,
            blocked_count: m.blocked_count,
            redacted_count: m.redacted_count,
            warned_count: m.warned_count,
            flagged_count: m.flagged_count,
            avg_latency_ms: m.avg_latency_ms,
            create_time: m.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptProtectionRuleVo {
    pub id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub rule_code: String,
    pub rule_name: String,
    pub pattern_type: String,
    pub phase: String,
    pub action: String,
    pub priority: i32,
    pub pattern_config: serde_json::Value,
    pub rewrite_template: String,
    pub status: PromptProtectionStatus,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl PromptProtectionRuleVo {
    pub fn from_model(m: prompt_protection_rule::Model) -> Self {
        Self {
            id: m.id,
            organization_id: m.organization_id,
            project_id: m.project_id,
            rule_code: m.rule_code,
            rule_name: m.rule_name,
            pattern_type: m.pattern_type,
            phase: m.phase,
            action: m.action,
            priority: m.priority,
            pattern_config: m.pattern_config,
            rewrite_template: m.rewrite_template,
            status: m.status,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
