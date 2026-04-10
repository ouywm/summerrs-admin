use chrono::{DateTime, FixedOffset, NaiveDate};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::guardrail_config;
use summer_ai_model::entity::guardrail_metric_daily;
use summer_ai_model::entity::guardrail_rule::{self, GuardrailRuleSeverity};
use summer_ai_model::entity::guardrail_violation;
use summer_ai_model::entity::prompt_protection_rule::{self, PromptProtectionRuleStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailConfigRes {
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
    pub create_by: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_by: String,
    pub update_time: DateTime<FixedOffset>,
}

impl GuardrailConfigRes {
    pub fn from_model(model: guardrail_config::Model) -> Self {
        Self {
            id: model.id,
            scope_type: model.scope_type,
            organization_id: model.organization_id,
            project_id: model.project_id,
            enabled: model.enabled,
            mode: model.mode,
            system_rules: model.system_rules,
            allowed_file_types: model.allowed_file_types,
            max_file_size_mb: model.max_file_size_mb,
            pii_action: model.pii_action,
            secret_action: model.secret_action,
            metadata: model.metadata,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailRuleRes {
    pub id: i64,
    pub guardrail_config_id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub team_id: i64,
    pub token_id: i64,
    pub service_account_id: i64,
    pub rule_code: String,
    pub rule_name: String,
    pub rule_type: String,
    pub phase: String,
    pub action: String,
    pub priority: i32,
    pub enabled: bool,
    pub severity: GuardrailRuleSeverity,
    pub model_pattern: String,
    pub endpoint_pattern: String,
    pub condition_json: serde_json::Value,
    pub rule_config: serde_json::Value,
    pub metadata: serde_json::Value,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_by: String,
    pub update_time: DateTime<FixedOffset>,
}

impl GuardrailRuleRes {
    pub fn from_model(model: guardrail_rule::Model) -> Self {
        Self {
            id: model.id,
            guardrail_config_id: model.guardrail_config_id,
            organization_id: model.organization_id,
            project_id: model.project_id,
            team_id: model.team_id,
            token_id: model.token_id,
            service_account_id: model.service_account_id,
            rule_code: model.rule_code,
            rule_name: model.rule_name,
            rule_type: model.rule_type,
            phase: model.phase,
            action: model.action,
            priority: model.priority,
            enabled: model.enabled,
            severity: model.severity,
            model_pattern: model.model_pattern,
            endpoint_pattern: model.endpoint_pattern,
            condition_json: model.condition_json,
            rule_config: model.rule_config,
            metadata: model.metadata,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailViolationRes {
    pub id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub user_id: i64,
    pub token_id: i64,
    pub service_account_id: i64,
    pub rule_id: i64,
    pub request_id: String,
    pub execution_id: i64,
    pub log_id: i64,
    pub task_id: i64,
    pub phase: String,
    pub category: String,
    pub action_taken: String,
    pub model_name: String,
    pub endpoint: String,
    pub matched_pattern: String,
    pub matched_content_hash: String,
    pub sample_excerpt: String,
    pub severity: i16,
    pub latency_ms: i32,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<FixedOffset>,
}

impl GuardrailViolationRes {
    pub fn from_model(model: guardrail_violation::Model) -> Self {
        Self {
            id: model.id,
            organization_id: model.organization_id,
            project_id: model.project_id,
            user_id: model.user_id,
            token_id: model.token_id,
            service_account_id: model.service_account_id,
            rule_id: model.rule_id,
            request_id: model.request_id,
            execution_id: model.execution_id,
            log_id: model.log_id,
            task_id: model.task_id,
            phase: model.phase,
            category: model.category,
            action_taken: model.action_taken,
            model_name: model.model_name,
            endpoint: model.endpoint,
            matched_pattern: model.matched_pattern,
            matched_content_hash: model.matched_content_hash,
            sample_excerpt: model.sample_excerpt,
            severity: model.severity,
            latency_ms: model.latency_ms,
            metadata: model.metadata,
            create_time: model.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptProtectionRuleRes {
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
    pub status: PromptProtectionRuleStatus,
    pub metadata: serde_json::Value,
    pub create_by: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_by: String,
    pub update_time: DateTime<FixedOffset>,
}

impl PromptProtectionRuleRes {
    pub fn from_model(model: prompt_protection_rule::Model) -> Self {
        Self {
            id: model.id,
            organization_id: model.organization_id,
            project_id: model.project_id,
            rule_code: model.rule_code,
            rule_name: model.rule_name,
            pattern_type: model.pattern_type,
            phase: model.phase,
            action: model.action,
            priority: model.priority,
            pattern_config: model.pattern_config,
            rewrite_template: model.rewrite_template,
            status: model.status,
            metadata: model.metadata,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailMetricDailyRes {
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

impl GuardrailMetricDailyRes {
    pub fn from_model(model: guardrail_metric_daily::Model) -> Self {
        Self {
            id: model.id,
            stats_date: model.stats_date,
            organization_id: model.organization_id,
            project_id: model.project_id,
            rule_id: model.rule_id,
            rule_code: model.rule_code,
            requests_evaluated: model.requests_evaluated,
            passed_count: model.passed_count,
            blocked_count: model.blocked_count,
            redacted_count: model.redacted_count,
            warned_count: model.warned_count,
            flagged_count: model.flagged_count,
            avg_latency_ms: model.avg_latency_ms,
            create_time: model.create_time,
        }
    }
}
