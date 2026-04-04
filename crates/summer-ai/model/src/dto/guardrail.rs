use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::guardrail_config;
use crate::entity::guardrail_rule::{self, GuardrailSeverity};
use crate::entity::prompt_protection_rule::{self, PromptProtectionStatus};

// ─── GuardrailConfig ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateGuardrailConfigDto {
    #[serde(default = "default_scope_type")]
    pub scope_type: String,
    #[serde(default)]
    pub organization_id: i64,
    #[serde(default)]
    pub project_id: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub system_rules: serde_json::Value,
    #[serde(default)]
    pub allowed_file_types: serde_json::Value,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: i32,
    #[serde(default = "default_pii_action")]
    pub pii_action: String,
    #[serde(default = "default_secret_action")]
    pub secret_action: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub remark: String,
}

fn default_scope_type() -> String {
    "organization".into()
}
fn default_true() -> bool {
    true
}
fn default_mode() -> String {
    "enforce".into()
}
fn default_max_file_size() -> i32 {
    20
}
fn default_pii_action() -> String {
    "redact".into()
}
fn default_secret_action() -> String {
    "block".into()
}

impl CreateGuardrailConfigDto {
    pub fn into_active_model(self, operator: &str) -> guardrail_config::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        guardrail_config::ActiveModel {
            scope_type: Set(self.scope_type),
            organization_id: Set(self.organization_id),
            project_id: Set(self.project_id),
            enabled: Set(self.enabled),
            mode: Set(self.mode),
            system_rules: Set(self.system_rules),
            allowed_file_types: Set(self.allowed_file_types),
            max_file_size_mb: Set(self.max_file_size_mb),
            pii_action: Set(self.pii_action),
            secret_action: Set(self.secret_action),
            metadata: Set(self.metadata),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGuardrailConfigDto {
    pub enabled: Option<bool>,
    pub mode: Option<String>,
    pub system_rules: Option<serde_json::Value>,
    pub allowed_file_types: Option<serde_json::Value>,
    pub max_file_size_mb: Option<i32>,
    pub pii_action: Option<String>,
    pub secret_action: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub remark: Option<String>,
}

impl UpdateGuardrailConfigDto {
    pub fn apply_to(self, active: &mut guardrail_config::ActiveModel, operator: &str) {
        if let Some(v) = self.enabled {
            active.enabled = Set(v);
        }
        if let Some(v) = self.mode {
            active.mode = Set(v);
        }
        if let Some(v) = self.system_rules {
            active.system_rules = Set(v);
        }
        if let Some(v) = self.allowed_file_types {
            active.allowed_file_types = Set(v);
        }
        if let Some(v) = self.max_file_size_mb {
            active.max_file_size_mb = Set(v);
        }
        if let Some(v) = self.pii_action {
            active.pii_action = Set(v);
        }
        if let Some(v) = self.secret_action {
            active.secret_action = Set(v);
        }
        if let Some(v) = self.metadata {
            active.metadata = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}

// ─── GuardrailRule ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateGuardrailRuleDto {
    pub guardrail_config_id: i64,
    #[serde(default)]
    pub organization_id: i64,
    #[serde(default)]
    pub project_id: i64,
    #[validate(length(min = 1, max = 64))]
    pub rule_code: String,
    pub rule_name: String,
    #[serde(default = "default_rule_type")]
    pub rule_type: String,
    #[serde(default = "default_phase")]
    pub phase: String,
    #[serde(default = "default_action")]
    pub action: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_guardrail_severity")]
    pub severity: GuardrailSeverity,
    #[serde(default = "default_wildcard")]
    pub model_pattern: String,
    #[serde(default = "default_wildcard")]
    pub endpoint_pattern: String,
    #[serde(default)]
    pub condition_json: serde_json::Value,
    #[serde(default)]
    pub rule_config: serde_json::Value,
    #[serde(default)]
    pub remark: String,
}

fn default_rule_type() -> String {
    "custom_regex".into()
}
fn default_phase() -> String {
    "request_input".into()
}
fn default_action() -> String {
    "block".into()
}
fn default_priority() -> i32 {
    100
}
fn default_guardrail_severity() -> GuardrailSeverity {
    GuardrailSeverity::Medium
}
fn default_wildcard() -> String {
    "*".into()
}

impl CreateGuardrailRuleDto {
    pub fn into_active_model(self, operator: &str) -> guardrail_rule::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        guardrail_rule::ActiveModel {
            guardrail_config_id: Set(self.guardrail_config_id),
            organization_id: Set(self.organization_id),
            project_id: Set(self.project_id),
            team_id: Set(0),
            token_id: Set(0),
            service_account_id: Set(0),
            rule_code: Set(self.rule_code),
            rule_name: Set(self.rule_name),
            rule_type: Set(self.rule_type),
            phase: Set(self.phase),
            action: Set(self.action),
            priority: Set(self.priority),
            enabled: Set(self.enabled),
            severity: Set(self.severity),
            model_pattern: Set(self.model_pattern),
            endpoint_pattern: Set(self.endpoint_pattern),
            condition_json: Set(self.condition_json),
            rule_config: Set(self.rule_config),
            metadata: Set(serde_json::json!({})),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGuardrailRuleDto {
    pub rule_name: Option<String>,
    pub rule_type: Option<String>,
    pub phase: Option<String>,
    pub action: Option<String>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
    pub severity: Option<GuardrailSeverity>,
    pub model_pattern: Option<String>,
    pub endpoint_pattern: Option<String>,
    pub condition_json: Option<serde_json::Value>,
    pub rule_config: Option<serde_json::Value>,
    pub remark: Option<String>,
}

impl UpdateGuardrailRuleDto {
    pub fn apply_to(self, active: &mut guardrail_rule::ActiveModel, operator: &str) {
        if let Some(v) = self.rule_name {
            active.rule_name = Set(v);
        }
        if let Some(v) = self.rule_type {
            active.rule_type = Set(v);
        }
        if let Some(v) = self.phase {
            active.phase = Set(v);
        }
        if let Some(v) = self.action {
            active.action = Set(v);
        }
        if let Some(v) = self.priority {
            active.priority = Set(v);
        }
        if let Some(v) = self.enabled {
            active.enabled = Set(v);
        }
        if let Some(v) = self.severity {
            active.severity = Set(v);
        }
        if let Some(v) = self.model_pattern {
            active.model_pattern = Set(v);
        }
        if let Some(v) = self.endpoint_pattern {
            active.endpoint_pattern = Set(v);
        }
        if let Some(v) = self.condition_json {
            active.condition_json = Set(v);
        }
        if let Some(v) = self.rule_config {
            active.rule_config = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryGuardrailRuleDto {
    pub guardrail_config_id: Option<i64>,
    pub rule_type: Option<String>,
    pub phase: Option<String>,
    pub enabled: Option<bool>,
}

impl From<QueryGuardrailRuleDto> for sea_orm::Condition {
    fn from(dto: QueryGuardrailRuleDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.guardrail_config_id {
            cond = cond.add(guardrail_rule::Column::GuardrailConfigId.eq(v));
        }
        if let Some(v) = dto.rule_type {
            cond = cond.add(guardrail_rule::Column::RuleType.eq(v));
        }
        if let Some(v) = dto.phase {
            cond = cond.add(guardrail_rule::Column::Phase.eq(v));
        }
        if let Some(v) = dto.enabled {
            cond = cond.add(guardrail_rule::Column::Enabled.eq(v));
        }
        cond
    }
}

// ─── Violation 查询 ───

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryGuardrailViolationDto {
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
    pub rule_id: Option<i64>,
    pub category: Option<String>,
    pub action_taken: Option<String>,
    pub request_id: Option<String>,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
}

impl From<QueryGuardrailViolationDto> for sea_orm::Condition {
    fn from(dto: QueryGuardrailViolationDto) -> Self {
        use crate::entity::guardrail_violation;
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.organization_id {
            cond = cond.add(guardrail_violation::Column::OrganizationId.eq(v));
        }
        if let Some(v) = dto.project_id {
            cond = cond.add(guardrail_violation::Column::ProjectId.eq(v));
        }
        if let Some(v) = dto.rule_id {
            cond = cond.add(guardrail_violation::Column::RuleId.eq(v));
        }
        if let Some(v) = dto.category {
            cond = cond.add(guardrail_violation::Column::Category.eq(v));
        }
        if let Some(v) = dto.action_taken {
            cond = cond.add(guardrail_violation::Column::ActionTaken.eq(v));
        }
        if let Some(v) = dto.request_id {
            cond = cond.add(guardrail_violation::Column::RequestId.eq(v));
        }
        if let Some(v) = dto.start_time {
            cond = cond.add(guardrail_violation::Column::CreateTime.gte(v));
        }
        if let Some(v) = dto.end_time {
            cond = cond.add(guardrail_violation::Column::CreateTime.lte(v));
        }
        cond
    }
}

// ─── PromptProtectionRule ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreatePromptProtectionRuleDto {
    #[serde(default)]
    pub organization_id: i64,
    #[serde(default)]
    pub project_id: i64,
    #[validate(length(min = 1, max = 64))]
    pub rule_code: String,
    pub rule_name: String,
    #[serde(default = "default_pattern_type")]
    pub pattern_type: String,
    #[serde(default = "default_phase")]
    pub phase: String,
    #[serde(default = "default_action")]
    pub action: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    pub pattern_config: serde_json::Value,
    #[serde(default)]
    pub rewrite_template: String,
}

fn default_pattern_type() -> String {
    "regex".into()
}

impl CreatePromptProtectionRuleDto {
    pub fn into_active_model(self, operator: &str) -> prompt_protection_rule::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        prompt_protection_rule::ActiveModel {
            organization_id: Set(self.organization_id),
            project_id: Set(self.project_id),
            rule_code: Set(self.rule_code),
            rule_name: Set(self.rule_name),
            pattern_type: Set(self.pattern_type),
            phase: Set(self.phase),
            action: Set(self.action),
            priority: Set(self.priority),
            pattern_config: Set(self.pattern_config),
            rewrite_template: Set(self.rewrite_template),
            status: Set(PromptProtectionStatus::Enabled),
            metadata: Set(serde_json::json!({})),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePromptProtectionRuleDto {
    pub rule_name: Option<String>,
    pub pattern_type: Option<String>,
    pub phase: Option<String>,
    pub action: Option<String>,
    pub priority: Option<i32>,
    pub pattern_config: Option<serde_json::Value>,
    pub rewrite_template: Option<String>,
    pub status: Option<PromptProtectionStatus>,
}

impl UpdatePromptProtectionRuleDto {
    pub fn apply_to(self, active: &mut prompt_protection_rule::ActiveModel, operator: &str) {
        if let Some(v) = self.rule_name {
            active.rule_name = Set(v);
        }
        if let Some(v) = self.pattern_type {
            active.pattern_type = Set(v);
        }
        if let Some(v) = self.phase {
            active.phase = Set(v);
        }
        if let Some(v) = self.action {
            active.action = Set(v);
        }
        if let Some(v) = self.priority {
            active.priority = Set(v);
        }
        if let Some(v) = self.pattern_config {
            active.pattern_config = Set(v);
        }
        if let Some(v) = self.rewrite_template {
            active.rewrite_template = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}
