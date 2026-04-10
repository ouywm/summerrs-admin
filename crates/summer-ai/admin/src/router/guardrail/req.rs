use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use summer_ai_model::entity::guardrail_config;
use summer_ai_model::entity::guardrail_rule::{self, GuardrailRuleSeverity};
use summer_ai_model::entity::guardrail_violation;
use summer_ai_model::entity::prompt_protection_rule::{self, PromptProtectionRuleStatus};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailRuleQuery {
    pub guardrail_config_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
    pub rule_code: Option<String>,
    pub rule_name: Option<String>,
    pub phase: Option<String>,
    pub action: Option<String>,
    pub enabled: Option<bool>,
    pub severity: Option<GuardrailRuleSeverity>,
}

impl From<GuardrailRuleQuery> for Condition {
    fn from(req: GuardrailRuleQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(guardrail_config_id) = req.guardrail_config_id {
            condition =
                condition.add(guardrail_rule::Column::GuardrailConfigId.eq(guardrail_config_id));
        }
        if let Some(organization_id) = req.organization_id {
            condition = condition.add(guardrail_rule::Column::OrganizationId.eq(organization_id));
        }
        if let Some(project_id) = req.project_id {
            condition = condition.add(guardrail_rule::Column::ProjectId.eq(project_id));
        }
        if let Some(rule_code) = req.rule_code {
            condition = condition.add(guardrail_rule::Column::RuleCode.contains(&rule_code));
        }
        if let Some(rule_name) = req.rule_name {
            condition = condition.add(guardrail_rule::Column::RuleName.contains(&rule_name));
        }
        if let Some(phase) = req.phase {
            condition = condition.add(guardrail_rule::Column::Phase.eq(phase));
        }
        if let Some(action) = req.action {
            condition = condition.add(guardrail_rule::Column::Action.eq(action));
        }
        if let Some(enabled) = req.enabled {
            condition = condition.add(guardrail_rule::Column::Enabled.eq(enabled));
        }
        if let Some(severity) = req.severity {
            condition = condition.add(guardrail_rule::Column::Severity.eq(severity));
        }
        condition
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailViolationQuery {
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
    pub user_id: Option<i64>,
    pub token_id: Option<i64>,
    pub service_account_id: Option<i64>,
    pub rule_id: Option<i64>,
    pub request_id: Option<String>,
    pub phase: Option<String>,
    pub category: Option<String>,
    pub action_taken: Option<String>,
    pub create_time_start: Option<DateTime<FixedOffset>>,
    pub create_time_end: Option<DateTime<FixedOffset>>,
}

impl From<GuardrailViolationQuery> for Condition {
    fn from(req: GuardrailViolationQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(organization_id) = req.organization_id {
            condition =
                condition.add(guardrail_violation::Column::OrganizationId.eq(organization_id));
        }
        if let Some(project_id) = req.project_id {
            condition = condition.add(guardrail_violation::Column::ProjectId.eq(project_id));
        }
        if let Some(user_id) = req.user_id {
            condition = condition.add(guardrail_violation::Column::UserId.eq(user_id));
        }
        if let Some(token_id) = req.token_id {
            condition = condition.add(guardrail_violation::Column::TokenId.eq(token_id));
        }
        if let Some(service_account_id) = req.service_account_id {
            condition =
                condition.add(guardrail_violation::Column::ServiceAccountId.eq(service_account_id));
        }
        if let Some(rule_id) = req.rule_id {
            condition = condition.add(guardrail_violation::Column::RuleId.eq(rule_id));
        }
        if let Some(request_id) = req.request_id {
            condition = condition.add(guardrail_violation::Column::RequestId.contains(&request_id));
        }
        if let Some(phase) = req.phase {
            condition = condition.add(guardrail_violation::Column::Phase.eq(phase));
        }
        if let Some(category) = req.category {
            condition = condition.add(guardrail_violation::Column::Category.eq(category));
        }
        if let Some(action_taken) = req.action_taken {
            condition = condition.add(guardrail_violation::Column::ActionTaken.eq(action_taken));
        }
        if let Some(create_time_start) = req.create_time_start {
            condition =
                condition.add(guardrail_violation::Column::CreateTime.gte(create_time_start));
        }
        if let Some(create_time_end) = req.create_time_end {
            condition = condition.add(guardrail_violation::Column::CreateTime.lte(create_time_end));
        }
        condition
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateGuardrailConfigReq {
    #[validate(length(min = 1, max = 32))]
    pub scope_type: String,
    #[serde(default)]
    pub organization_id: i64,
    #[serde(default)]
    pub project_id: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[validate(length(min = 1, max = 16))]
    pub mode: String,
    #[serde(default)]
    pub system_rules: serde_json::Value,
    #[serde(default)]
    pub allowed_file_types: serde_json::Value,
    #[serde(default = "default_max_file_size_mb")]
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGuardrailConfigReq {
    pub scope_type: Option<String>,
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateGuardrailRuleReq {
    pub guardrail_config_id: i64,
    #[serde(default)]
    pub organization_id: i64,
    #[serde(default)]
    pub project_id: i64,
    #[serde(default)]
    pub team_id: i64,
    #[serde(default)]
    pub token_id: i64,
    #[serde(default)]
    pub service_account_id: i64,
    #[validate(length(min = 1, max = 64))]
    pub rule_code: String,
    #[validate(length(min = 1, max = 128))]
    pub rule_name: String,
    #[validate(length(min = 1, max = 64))]
    pub rule_type: String,
    #[validate(length(min = 1, max = 64))]
    pub phase: String,
    #[validate(length(min = 1, max = 32))]
    pub action: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_guardrail_rule_severity")]
    pub severity: GuardrailRuleSeverity,
    #[serde(default = "default_wildcard")]
    pub model_pattern: String,
    #[serde(default = "default_wildcard")]
    pub endpoint_pattern: String,
    #[serde(default)]
    pub condition_json: serde_json::Value,
    #[serde(default)]
    pub rule_config: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub remark: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGuardrailRuleReq {
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
    pub team_id: Option<i64>,
    pub token_id: Option<i64>,
    pub service_account_id: Option<i64>,
    pub rule_code: Option<String>,
    pub rule_name: Option<String>,
    pub rule_type: Option<String>,
    pub phase: Option<String>,
    pub action: Option<String>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
    pub severity: Option<GuardrailRuleSeverity>,
    pub model_pattern: Option<String>,
    pub endpoint_pattern: Option<String>,
    pub condition_json: Option<serde_json::Value>,
    pub rule_config: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreatePromptProtectionRuleReq {
    #[serde(default)]
    pub organization_id: i64,
    #[serde(default)]
    pub project_id: i64,
    #[validate(length(min = 1, max = 64))]
    pub rule_code: String,
    #[validate(length(min = 1, max = 128))]
    pub rule_name: String,
    #[validate(length(min = 1, max = 32))]
    pub pattern_type: String,
    #[validate(length(min = 1, max = 64))]
    pub phase: String,
    #[validate(length(min = 1, max = 32))]
    pub action: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub pattern_config: serde_json::Value,
    #[serde(default)]
    pub rewrite_template: String,
    #[serde(default = "default_prompt_rule_status")]
    pub status: PromptProtectionRuleStatus,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePromptProtectionRuleReq {
    pub organization_id: Option<i64>,
    pub project_id: Option<i64>,
    pub rule_code: Option<String>,
    pub rule_name: Option<String>,
    pub pattern_type: Option<String>,
    pub phase: Option<String>,
    pub action: Option<String>,
    pub priority: Option<i32>,
    pub pattern_config: Option<serde_json::Value>,
    pub rewrite_template: Option<String>,
    pub status: Option<PromptProtectionRuleStatus>,
    pub metadata: Option<serde_json::Value>,
}

impl CreateGuardrailConfigReq {
    pub fn into_active_model(self, operator: &str) -> guardrail_config::ActiveModel {
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
            ..Default::default()
        }
    }
}

impl UpdateGuardrailConfigReq {
    pub fn apply_to(self, active: &mut guardrail_config::ActiveModel, operator: &str) {
        if let Some(scope_type) = self.scope_type {
            active.scope_type = Set(scope_type);
        }
        if let Some(organization_id) = self.organization_id {
            active.organization_id = Set(organization_id);
        }
        if let Some(project_id) = self.project_id {
            active.project_id = Set(project_id);
        }
        if let Some(enabled) = self.enabled {
            active.enabled = Set(enabled);
        }
        if let Some(mode) = self.mode {
            active.mode = Set(mode);
        }
        if let Some(system_rules) = self.system_rules {
            active.system_rules = Set(system_rules);
        }
        if let Some(allowed_file_types) = self.allowed_file_types {
            active.allowed_file_types = Set(allowed_file_types);
        }
        if let Some(max_file_size_mb) = self.max_file_size_mb {
            active.max_file_size_mb = Set(max_file_size_mb);
        }
        if let Some(pii_action) = self.pii_action {
            active.pii_action = Set(pii_action);
        }
        if let Some(secret_action) = self.secret_action {
            active.secret_action = Set(secret_action);
        }
        if let Some(metadata) = self.metadata {
            active.metadata = Set(metadata);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
        active.update_by = Set(operator.to_string());
    }
}

impl CreateGuardrailRuleReq {
    pub fn into_active_model(self, operator: &str) -> guardrail_rule::ActiveModel {
        guardrail_rule::ActiveModel {
            guardrail_config_id: Set(self.guardrail_config_id),
            organization_id: Set(self.organization_id),
            project_id: Set(self.project_id),
            team_id: Set(self.team_id),
            token_id: Set(self.token_id),
            service_account_id: Set(self.service_account_id),
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
            metadata: Set(self.metadata),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }
}

impl UpdateGuardrailRuleReq {
    pub fn apply_to(self, active: &mut guardrail_rule::ActiveModel, operator: &str) {
        if let Some(organization_id) = self.organization_id {
            active.organization_id = Set(organization_id);
        }
        if let Some(project_id) = self.project_id {
            active.project_id = Set(project_id);
        }
        if let Some(team_id) = self.team_id {
            active.team_id = Set(team_id);
        }
        if let Some(token_id) = self.token_id {
            active.token_id = Set(token_id);
        }
        if let Some(service_account_id) = self.service_account_id {
            active.service_account_id = Set(service_account_id);
        }
        if let Some(rule_code) = self.rule_code {
            active.rule_code = Set(rule_code);
        }
        if let Some(rule_name) = self.rule_name {
            active.rule_name = Set(rule_name);
        }
        if let Some(rule_type) = self.rule_type {
            active.rule_type = Set(rule_type);
        }
        if let Some(phase) = self.phase {
            active.phase = Set(phase);
        }
        if let Some(action) = self.action {
            active.action = Set(action);
        }
        if let Some(priority) = self.priority {
            active.priority = Set(priority);
        }
        if let Some(enabled) = self.enabled {
            active.enabled = Set(enabled);
        }
        if let Some(severity) = self.severity {
            active.severity = Set(severity);
        }
        if let Some(model_pattern) = self.model_pattern {
            active.model_pattern = Set(model_pattern);
        }
        if let Some(endpoint_pattern) = self.endpoint_pattern {
            active.endpoint_pattern = Set(endpoint_pattern);
        }
        if let Some(condition_json) = self.condition_json {
            active.condition_json = Set(condition_json);
        }
        if let Some(rule_config) = self.rule_config {
            active.rule_config = Set(rule_config);
        }
        if let Some(metadata) = self.metadata {
            active.metadata = Set(metadata);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
        active.update_by = Set(operator.to_string());
    }
}

impl CreatePromptProtectionRuleReq {
    pub fn into_active_model(self, operator: &str) -> prompt_protection_rule::ActiveModel {
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
            status: Set(self.status),
            metadata: Set(self.metadata),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }
}

impl UpdatePromptProtectionRuleReq {
    pub fn apply_to(self, active: &mut prompt_protection_rule::ActiveModel, operator: &str) {
        if let Some(organization_id) = self.organization_id {
            active.organization_id = Set(organization_id);
        }
        if let Some(project_id) = self.project_id {
            active.project_id = Set(project_id);
        }
        if let Some(rule_code) = self.rule_code {
            active.rule_code = Set(rule_code);
        }
        if let Some(rule_name) = self.rule_name {
            active.rule_name = Set(rule_name);
        }
        if let Some(pattern_type) = self.pattern_type {
            active.pattern_type = Set(pattern_type);
        }
        if let Some(phase) = self.phase {
            active.phase = Set(phase);
        }
        if let Some(action) = self.action {
            active.action = Set(action);
        }
        if let Some(priority) = self.priority {
            active.priority = Set(priority);
        }
        if let Some(pattern_config) = self.pattern_config {
            active.pattern_config = Set(pattern_config);
        }
        if let Some(rewrite_template) = self.rewrite_template {
            active.rewrite_template = Set(rewrite_template);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        if let Some(metadata) = self.metadata {
            active.metadata = Set(metadata);
        }
        active.update_by = Set(operator.to_string());
    }
}

fn default_true() -> bool {
    true
}

fn default_priority() -> i32 {
    100
}

fn default_max_file_size_mb() -> i32 {
    20
}

fn default_pii_action() -> String {
    "redact".to_string()
}

fn default_secret_action() -> String {
    "block".to_string()
}

fn default_wildcard() -> String {
    "*".to_string()
}

fn default_guardrail_rule_severity() -> GuardrailRuleSeverity {
    GuardrailRuleSeverity::Medium
}

fn default_prompt_rule_status() -> PromptProtectionRuleStatus {
    PromptProtectionRuleStatus::Enabled
}
