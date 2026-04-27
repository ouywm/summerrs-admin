use crate::entity::routing::ability;
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateAbilityDto {
    #[validate(length(min = 1, max = 64, message = "渠道分组长度必须在1-64之间"))]
    pub channel_group: String,
    #[validate(length(min = 1, max = 32, message = "endpoint 范围长度必须在1-32之间"))]
    pub endpoint_scope: String,
    #[validate(length(min = 1, max = 128, message = "模型名长度必须在1-128之间"))]
    pub model: String,
    pub channel_id: i64,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub weight: Option<i32>,
    pub route_config: Option<serde_json::Value>,
}

impl CreateAbilityDto {
    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_channel_id(self.channel_id)
    }

    pub fn into_active_model(self) -> Result<ability::ActiveModel, String> {
        self.validate_business_rules()?;
        Ok(ability::ActiveModel {
            channel_group: Set(self.channel_group),
            endpoint_scope: Set(self.endpoint_scope),
            model: Set(self.model),
            channel_id: Set(self.channel_id),
            enabled: Set(self.enabled.unwrap_or(true)),
            priority: Set(self.priority.unwrap_or(0)),
            weight: Set(self.weight.unwrap_or(100)),
            route_config: Set(self.route_config.unwrap_or_else(|| serde_json::json!({}))),
            ..Default::default()
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAbilityDto {
    #[validate(length(min = 1, max = 64, message = "渠道分组长度必须在1-64之间"))]
    pub channel_group: Option<String>,
    #[validate(length(min = 1, max = 32, message = "endpoint 范围长度必须在1-32之间"))]
    pub endpoint_scope: Option<String>,
    #[validate(length(min = 1, max = 128, message = "模型名长度必须在1-128之间"))]
    pub model: Option<String>,
    pub channel_id: Option<i64>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub weight: Option<i32>,
    pub route_config: Option<serde_json::Value>,
}

impl UpdateAbilityDto {
    pub fn validate_business_rules(&self, current: &ability::Model) -> Result<(), String> {
        validate_channel_id(self.channel_id.unwrap_or(current.channel_id))
    }

    pub fn apply_to(self, active: &mut ability::ActiveModel) -> Result<(), String> {
        if let Some(v) = self.channel_group {
            active.channel_group = Set(v);
        }
        if let Some(v) = self.endpoint_scope {
            active.endpoint_scope = Set(v);
        }
        if let Some(v) = self.model {
            active.model = Set(v);
        }
        if let Some(v) = self.channel_id {
            validate_channel_id(v)?;
            active.channel_id = Set(v);
        }
        if let Some(v) = self.enabled {
            active.enabled = Set(v);
        }
        if let Some(v) = self.priority {
            active.priority = Set(v);
        }
        if let Some(v) = self.weight {
            active.weight = Set(v);
        }
        if let Some(v) = self.route_config {
            active.route_config = Set(v);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AbilityQueryDto {
    pub channel_group: Option<String>,
    pub endpoint_scope: Option<String>,
    pub model: Option<String>,
    pub channel_id: Option<i64>,
    pub enabled: Option<bool>,
    pub keyword: Option<String>,
}

impl From<AbilityQueryDto> for Condition {
    fn from(query: AbilityQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.channel_group {
            cond = cond.add(ability::Column::ChannelGroup.eq(v));
        }
        if let Some(v) = query.endpoint_scope {
            cond = cond.add(ability::Column::EndpointScope.eq(v));
        }
        if let Some(v) = query.model {
            cond = cond.add(ability::Column::Model.eq(v));
        }
        if let Some(v) = query.channel_id {
            cond = cond.add(ability::Column::ChannelId.eq(v));
        }
        if let Some(v) = query.enabled {
            cond = cond.add(ability::Column::Enabled.eq(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(ability::Column::ChannelGroup.contains(&keyword))
                        .add(ability::Column::EndpointScope.contains(&keyword))
                        .add(ability::Column::Model.contains(&keyword)),
                );
            }
        }
        cond
    }
}

fn validate_channel_id(channel_id: i64) -> Result<(), String> {
    if channel_id <= 0 {
        return Err("channelId 必须大于 0".to_string());
    }
    Ok(())
}
