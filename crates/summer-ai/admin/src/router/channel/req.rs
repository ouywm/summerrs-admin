use std::collections::HashSet;

use schemars::JsonSchema;
use sea_orm::prelude::BigDecimal;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use validator::Validate;

use summer_ai_model::entity::channel::{self, ChannelLastHealthStatus, ChannelStatus, ChannelType};

pub const KNOWN_ENDPOINT_SCOPES: &[&str] = &[
    "chat",
    "completions",
    "responses",
    "embeddings",
    "images",
    "audio",
    "moderations",
    "rerank",
    "files",
    "batches",
    "assistants",
    "threads",
    "vector_stores",
    "fine_tuning",
    "uploads",
    "models",
];

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelQuery {
    pub name: Option<String>,
    pub vendor_code: Option<String>,
    pub status: Option<ChannelStatus>,
    pub channel_type: Option<ChannelType>,
    pub channel_group: Option<String>,
}

impl From<ChannelQuery> for Condition {
    fn from(req: ChannelQuery) -> Self {
        let mut condition = Condition::all().add(channel::Column::DeletedAt.is_null());
        if let Some(name) = req.name {
            condition = condition.add(channel::Column::Name.contains(&name));
        }
        if let Some(vendor_code) = req.vendor_code {
            condition = condition.add(channel::Column::VendorCode.eq(vendor_code));
        }
        if let Some(status) = req.status {
            condition = condition.add(channel::Column::Status.eq(status));
        }
        if let Some(channel_type) = req.channel_type {
            condition = condition.add(channel::Column::ChannelType.eq(channel_type));
        }
        if let Some(channel_group) = req.channel_group {
            condition = condition.add(channel::Column::ChannelGroup.eq(channel_group));
        }
        condition
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelReq {
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    pub channel_type: ChannelType,
    #[validate(length(max = 64))]
    pub vendor_code: String,
    #[validate(url)]
    pub base_url: String,
    pub models: Value,
    #[serde(default)]
    pub model_mapping: Value,
    #[validate(length(min = 1, max = 64))]
    pub channel_group: String,
    #[serde(default = "default_endpoint_scope_array")]
    pub endpoint_scopes: Value,
    #[serde(default)]
    pub capabilities: Value,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub config: Value,
    #[serde(default = "default_true")]
    pub auto_ban: bool,
    #[serde(default)]
    pub test_model: String,
    #[serde(default)]
    pub remark: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelReq {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    #[validate(length(max = 64))]
    pub vendor_code: Option<String>,
    #[validate(url)]
    pub base_url: Option<String>,
    pub status: Option<ChannelStatus>,
    pub models: Option<Value>,
    pub model_mapping: Option<Value>,
    pub channel_group: Option<String>,
    pub endpoint_scopes: Option<Value>,
    pub capabilities: Option<Value>,
    pub weight: Option<i32>,
    pub priority: Option<i32>,
    pub config: Option<Value>,
    pub auto_ban: Option<bool>,
    pub test_model: Option<String>,
    pub remark: Option<String>,
}

fn default_endpoint_scope_array() -> Value {
    Value::Array(Vec::new())
}

fn default_weight() -> i32 {
    1
}

fn default_true() -> bool {
    true
}

fn normalize_endpoint_scope_list(
    value: &Value,
    field_name: &'static str,
) -> Result<Vec<String>, String> {
    let items = match value {
        Value::Null => return Ok(Vec::new()),
        Value::Array(items) => items,
        _ => return Err(format!("{field_name} must be an array of strings")),
    };

    let mut scopes = Vec::with_capacity(items.len());
    let mut seen = HashSet::with_capacity(items.len());

    for item in items {
        let Some(scope) = item.as_str() else {
            return Err(format!("{field_name} must be an array of strings"));
        };

        let scope = scope.trim().to_ascii_lowercase();
        if scope.is_empty() {
            return Err(format!("{field_name} contains an empty endpoint scope"));
        }
        if !KNOWN_ENDPOINT_SCOPES.contains(&scope.as_str()) {
            return Err(format!(
                "unsupported endpoint scope in {field_name}: {scope}"
            ));
        }
        if seen.insert(scope.clone()) {
            scopes.push(scope);
        }
    }

    Ok(scopes)
}

fn normalize_endpoint_scope_value(value: Value, field_name: &'static str) -> Result<Value, String> {
    Ok(Value::Array(
        normalize_endpoint_scope_list(&value, field_name)?
            .into_iter()
            .map(Value::String)
            .collect(),
    ))
}

impl CreateChannelReq {
    pub fn into_active_model(self, operator: &str) -> Result<channel::ActiveModel, String> {
        Ok(channel::ActiveModel {
            name: Set(self.name),
            channel_type: Set(self.channel_type),
            vendor_code: Set(self.vendor_code),
            base_url: Set(self.base_url),
            status: Set(ChannelStatus::Enabled),
            models: Set(self.models),
            model_mapping: Set(self.model_mapping),
            channel_group: Set(self.channel_group),
            endpoint_scopes: Set(normalize_endpoint_scope_value(
                self.endpoint_scopes,
                "endpointScopes",
            )?),
            capabilities: Set(self.capabilities),
            weight: Set(self.weight),
            priority: Set(self.priority),
            config: Set(self.config),
            auto_ban: Set(self.auto_ban),
            test_model: Set(self.test_model),
            used_quota: Set(0),
            balance: Set(BigDecimal::from(0)),
            balance_updated_at: Set(None),
            response_time: Set(0),
            success_rate: Set(BigDecimal::from(0)),
            failure_streak: Set(0),
            last_used_at: Set(None),
            last_error_at: Set(None),
            last_error_code: Set(String::new()),
            last_error_message: Set(String::new()),
            last_health_status: Set(ChannelLastHealthStatus::Unknown),
            deleted_at: Set(None),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        })
    }
}

impl UpdateChannelReq {
    pub fn apply_to(self, active: &mut channel::ActiveModel, operator: &str) -> Result<(), String> {
        if let Some(name) = self.name {
            active.name = Set(name);
        }
        if let Some(vendor_code) = self.vendor_code {
            active.vendor_code = Set(vendor_code);
        }
        if let Some(base_url) = self.base_url {
            active.base_url = Set(base_url);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        if let Some(models) = self.models {
            active.models = Set(models);
        }
        if let Some(model_mapping) = self.model_mapping {
            active.model_mapping = Set(model_mapping);
        }
        if let Some(channel_group) = self.channel_group {
            active.channel_group = Set(channel_group);
        }
        if let Some(endpoint_scopes) = self.endpoint_scopes {
            active.endpoint_scopes = Set(normalize_endpoint_scope_value(
                endpoint_scopes,
                "endpointScopes",
            )?);
        }
        if let Some(capabilities) = self.capabilities {
            active.capabilities = Set(capabilities);
        }
        if let Some(weight) = self.weight {
            active.weight = Set(weight);
        }
        if let Some(priority) = self.priority {
            active.priority = Set(priority);
        }
        if let Some(config) = self.config {
            active.config = Set(config);
        }
        if let Some(auto_ban) = self.auto_ban {
            active.auto_ban = Set(auto_ban);
        }
        if let Some(test_model) = self.test_model {
            active.test_model = Set(test_model);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
        active.update_by = Set(operator.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_endpoint_scope_array, normalize_endpoint_scope_list, normalize_endpoint_scope_value,
    };

    #[test]
    fn default_endpoint_scope_array_returns_empty_json_array() {
        assert_eq!(default_endpoint_scope_array(), serde_json::json!([]));
    }

    #[test]
    fn normalize_endpoint_scope_list_trims_lowercases_and_deduplicates() {
        assert_eq!(
            normalize_endpoint_scope_list(
                &serde_json::json!([" Chat ", "responses", "chat", "THREADS"]),
                "endpointScopes"
            )
            .unwrap(),
            vec![
                "chat".to_string(),
                "responses".to_string(),
                "threads".to_string()
            ]
        );
    }

    #[test]
    fn normalize_endpoint_scope_list_rejects_unknown_scope() {
        let error =
            normalize_endpoint_scope_list(&serde_json::json!(["chat", "foo"]), "endpointScopes")
                .unwrap_err();
        assert_eq!(error, "unsupported endpoint scope in endpointScopes: foo");
    }

    #[test]
    fn normalize_endpoint_scope_value_returns_normalized_json_array() {
        assert_eq!(
            normalize_endpoint_scope_value(
                serde_json::json!(["responses", " CHAT ", "responses"]),
                "supportedEndpoints"
            )
            .unwrap(),
            serde_json::json!(["responses", "chat"])
        );
    }
}
