use crate::entity::routing::routing_target::{self, RoutingTargetStatus};
use schemars::JsonSchema;
use sea_orm::{ActiveValue, ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRoutingTargetBinding {
    pub target_type: String,
    pub channel_id: i64,
    pub account_id: i64,
    pub plugin_id: i64,
    pub target_key: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoutingTargetDto {
    pub routing_rule_id: i64,
    #[validate(length(min = 1, max = 32, message = "目标类型长度必须在1-32之间"))]
    pub target_type: String,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    pub plugin_id: Option<i64>,
    #[validate(length(max = 128, message = "targetKey 长度不能超过128"))]
    pub target_key: Option<String>,
    pub weight: Option<i32>,
    pub priority: Option<i32>,
    pub cooldown_seconds: Option<i32>,
    pub config: Option<serde_json::Value>,
    pub status: Option<RoutingTargetStatus>,
}

impl CreateRoutingTargetDto {
    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_routing_rule_id(self.routing_rule_id)?;
        validate_non_negative("weight", self.weight)?;
        validate_non_negative("priority", self.priority)?;
        validate_non_negative("cooldownSeconds", self.cooldown_seconds)?;
        self.normalized_binding()?;
        Ok(())
    }

    pub fn normalized_binding(&self) -> Result<NormalizedRoutingTargetBinding, String> {
        normalize_routing_target_binding(
            &self.target_type,
            self.channel_id.unwrap_or(0),
            self.account_id.unwrap_or(0),
            self.plugin_id.unwrap_or(0),
            self.target_key.as_deref().unwrap_or(""),
        )
    }

    pub fn into_active_model(self) -> Result<routing_target::ActiveModel, String> {
        let binding = self.normalized_binding()?;
        Ok(routing_target::ActiveModel {
            id: NotSet,
            routing_rule_id: Set(self.routing_rule_id),
            target_type: Set(binding.target_type),
            channel_id: Set(binding.channel_id),
            account_id: Set(binding.account_id),
            plugin_id: Set(binding.plugin_id),
            target_key: Set(binding.target_key),
            weight: Set(self.weight.unwrap_or(100)),
            priority: Set(self.priority.unwrap_or(0)),
            cooldown_seconds: Set(self.cooldown_seconds.unwrap_or(0)),
            config: Set(self.config.unwrap_or_else(|| serde_json::json!({}))),
            status: Set(self.status.unwrap_or(RoutingTargetStatus::Enabled)),
            create_time: NotSet,
            update_time: NotSet,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoutingTargetDto {
    pub routing_rule_id: Option<i64>,
    #[validate(length(min = 1, max = 32, message = "目标类型长度必须在1-32之间"))]
    pub target_type: Option<String>,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    pub plugin_id: Option<i64>,
    #[validate(length(max = 128, message = "targetKey 长度不能超过128"))]
    pub target_key: Option<String>,
    pub weight: Option<i32>,
    pub priority: Option<i32>,
    pub cooldown_seconds: Option<i32>,
    pub config: Option<serde_json::Value>,
    pub status: Option<RoutingTargetStatus>,
}

impl UpdateRoutingTargetDto {
    pub fn validate_business_rules(&self, current: &routing_target::Model) -> Result<(), String> {
        validate_routing_rule_id(self.routing_rule_id.unwrap_or(current.routing_rule_id))?;
        validate_non_negative("weight", self.weight)?;
        validate_non_negative("priority", self.priority)?;
        validate_non_negative("cooldownSeconds", self.cooldown_seconds)?;
        self.merged_binding(current)?;
        Ok(())
    }

    pub fn merged_binding(
        &self,
        current: &routing_target::Model,
    ) -> Result<NormalizedRoutingTargetBinding, String> {
        normalize_routing_target_binding(
            self.target_type
                .as_deref()
                .unwrap_or(current.target_type.as_str()),
            self.channel_id.unwrap_or(current.channel_id),
            self.account_id.unwrap_or(current.account_id),
            self.plugin_id.unwrap_or(current.plugin_id),
            self.target_key
                .as_deref()
                .unwrap_or(current.target_key.as_str()),
        )
    }

    pub fn apply_to(self, active: &mut routing_target::ActiveModel) -> Result<(), String> {
        let binding = normalize_routing_target_binding(
            self.target_type
                .as_deref()
                .unwrap_or(active_value_string(&active.target_type).as_str()),
            self.channel_id
                .unwrap_or_else(|| active_value_i64(&active.channel_id)),
            self.account_id
                .unwrap_or_else(|| active_value_i64(&active.account_id)),
            self.plugin_id
                .unwrap_or_else(|| active_value_i64(&active.plugin_id)),
            self.target_key
                .as_deref()
                .unwrap_or(active_value_string(&active.target_key).as_str()),
        )?;

        if let Some(v) = self.routing_rule_id {
            active.routing_rule_id = Set(v);
        }
        active.target_type = Set(binding.target_type);
        active.channel_id = Set(binding.channel_id);
        active.account_id = Set(binding.account_id);
        active.plugin_id = Set(binding.plugin_id);
        active.target_key = Set(binding.target_key);
        if let Some(v) = self.weight {
            active.weight = Set(v);
        }
        if let Some(v) = self.priority {
            active.priority = Set(v);
        }
        if let Some(v) = self.cooldown_seconds {
            active.cooldown_seconds = Set(v);
        }
        if let Some(v) = self.config {
            active.config = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoutingTargetQueryDto {
    pub routing_rule_id: Option<i64>,
    pub target_type: Option<String>,
    pub status: Option<RoutingTargetStatus>,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    pub plugin_id: Option<i64>,
    pub keyword: Option<String>,
}

impl From<RoutingTargetQueryDto> for Condition {
    fn from(query: RoutingTargetQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.routing_rule_id {
            cond = cond.add(routing_target::Column::RoutingRuleId.eq(v));
        }
        if let Some(v) = query.target_type {
            cond = cond.add(routing_target::Column::TargetType.eq(v.trim().to_ascii_lowercase()));
        }
        if let Some(v) = query.status {
            cond = cond.add(routing_target::Column::Status.eq(v));
        }
        if let Some(v) = query.channel_id {
            cond = cond.add(routing_target::Column::ChannelId.eq(v));
        }
        if let Some(v) = query.account_id {
            cond = cond.add(routing_target::Column::AccountId.eq(v));
        }
        if let Some(v) = query.plugin_id {
            cond = cond.add(routing_target::Column::PluginId.eq(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(routing_target::Column::TargetType.contains(&keyword))
                        .add(routing_target::Column::TargetKey.contains(&keyword)),
                );
            }
        }
        cond
    }
}

pub fn normalize_routing_target_binding(
    target_type: &str,
    channel_id: i64,
    account_id: i64,
    plugin_id: i64,
    target_key: &str,
) -> Result<NormalizedRoutingTargetBinding, String> {
    let normalized_type = normalize_target_type(target_type)?;
    let trimmed_key = target_key.trim();
    let binding = match normalized_type.as_str() {
        "channel" => {
            if channel_id <= 0 {
                return Err("channel 类型必须提供有效的 channelId".to_string());
            }
            NormalizedRoutingTargetBinding {
                target_type: normalized_type,
                channel_id,
                account_id: 0,
                plugin_id: 0,
                target_key: String::new(),
            }
        }
        "account" => {
            if account_id <= 0 {
                return Err("account 类型必须提供有效的 accountId".to_string());
            }
            NormalizedRoutingTargetBinding {
                target_type: normalized_type,
                channel_id: 0,
                account_id,
                plugin_id: 0,
                target_key: String::new(),
            }
        }
        "plugin" => {
            if plugin_id <= 0 {
                return Err("plugin 类型必须提供有效的 pluginId".to_string());
            }
            NormalizedRoutingTargetBinding {
                target_type: normalized_type,
                channel_id: 0,
                account_id: 0,
                plugin_id,
                target_key: String::new(),
            }
        }
        "channel_group" | "pipeline" => {
            if trimmed_key.is_empty() {
                return Err(format!("{normalized_type} 类型必须提供非空 targetKey"));
            }
            NormalizedRoutingTargetBinding {
                target_type: normalized_type,
                channel_id: 0,
                account_id: 0,
                plugin_id: 0,
                target_key: trimmed_key.to_string(),
            }
        }
        _ => unreachable!(),
    };
    Ok(binding)
}

fn normalize_target_type(target_type: &str) -> Result<String, String> {
    let normalized = target_type.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "channel" | "account" | "channel_group" | "plugin" | "pipeline" => Ok(normalized),
        _ => Err(format!("不支持的 targetType: {target_type}")),
    }
}

fn validate_routing_rule_id(routing_rule_id: i64) -> Result<(), String> {
    if routing_rule_id <= 0 {
        return Err("routingRuleId 必须大于 0".to_string());
    }
    Ok(())
}

fn validate_non_negative(field: &str, value: Option<i32>) -> Result<(), String> {
    if value.is_some_and(|v| v < 0) {
        return Err(format!("{field} 不能为负数"));
    }
    Ok(())
}

fn active_value_i64(value: &ActiveValue<i64>) -> i64 {
    match value {
        ActiveValue::Set(v) | ActiveValue::Unchanged(v) => *v,
        ActiveValue::NotSet => 0,
    }
}

fn active_value_string(value: &ActiveValue<String>) -> String {
    match value {
        ActiveValue::Set(v) | ActiveValue::Unchanged(v) => v.clone(),
        ActiveValue::NotSet => String::new(),
    }
}
