use crate::entity::routing::channel::{self, ChannelLastHealthStatus, ChannelStatus, ChannelType};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// CreateChannelDto
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelDto {
    #[validate(length(min = 1, max = 128, message = "渠道名称长度必须在1-128之间"))]
    pub name: String,
    pub channel_type: ChannelType,
    #[validate(length(min = 1, max = 64, message = "供应商编码长度必须在1-64之间"))]
    pub vendor_code: String,
    #[validate(length(min = 1, max = 512, message = "上游API基础地址长度必须在1-512之间"))]
    pub base_url: String,
    pub models: Option<Vec<String>>,
    pub model_mapping: Option<serde_json::Map<String, serde_json::Value>>,
    pub channel_group: Option<String>,
    pub endpoint_scopes: Option<Vec<String>>,
    pub capabilities: Option<Vec<String>>,
    pub weight: Option<i32>,
    pub priority: Option<i32>,
    pub config: Option<serde_json::Map<String, serde_json::Value>>,
    pub auto_ban: Option<bool>,
    #[validate(length(max = 128, message = "测速模型名长度不能超过128"))]
    pub test_model: Option<String>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
    pub status: Option<ChannelStatus>,
}

impl CreateChannelDto {
    pub fn into_active_model(self, operator: &str) -> channel::ActiveModel {
        channel::ActiveModel {
            id: NotSet,
            name: Set(self.name),
            channel_type: Set(self.channel_type),
            vendor_code: Set(self.vendor_code),
            base_url: Set(self.base_url),
            status: Set(self.status.unwrap_or(ChannelStatus::Enabled)),
            models: Set(self
                .models
                .map(|v| serde_json::to_value(v).unwrap())
                .unwrap_or_else(|| serde_json::json!([]))),
            model_mapping: Set(self
                .model_mapping
                .map(serde_json::Value::Object)
                .unwrap_or_else(|| serde_json::json!({}))),
            channel_group: Set(self.channel_group.unwrap_or_else(|| "default".to_string())),
            endpoint_scopes: Set(self
                .endpoint_scopes
                .map(|v| serde_json::to_value(v).unwrap())
                .unwrap_or_else(|| serde_json::json!([]))),
            capabilities: Set(self
                .capabilities
                .map(|v| serde_json::to_value(v).unwrap())
                .unwrap_or_else(|| serde_json::json!([]))),
            weight: Set(self.weight.unwrap_or(100)),
            priority: Set(self.priority.unwrap_or(1)),
            config: Set(self
                .config
                .map(serde_json::Value::Object)
                .unwrap_or_else(|| serde_json::json!({}))),
            auto_ban: Set(self.auto_ban.unwrap_or(true)),
            test_model: Set(self.test_model.unwrap_or_default()),
            used_quota: Set(0),
            balance: Set(bigdecimal::BigDecimal::from(0)),
            balance_updated_at: Set(None),
            response_time: Set(0),
            success_rate: Set(bigdecimal::BigDecimal::from(0)),
            failure_streak: Set(0),
            last_used_at: Set(None),
            last_error_at: Set(None),
            last_error_code: Set(String::new()),
            last_error_message: Set(String::new()),
            last_health_status: Set(ChannelLastHealthStatus::Unknown),
            deleted_at: Set(None),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            create_time: NotSet,
            update_by: Set(operator.to_string()),
            update_time: NotSet,
        }
    }
}

// ---------------------------------------------------------------------------
// UpdateChannelDto
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelDto {
    #[validate(length(min = 1, max = 128, message = "渠道名称长度必须在1-128之间"))]
    pub name: Option<String>,
    pub channel_type: Option<ChannelType>,
    #[validate(length(min = 1, max = 64, message = "供应商编码长度必须在1-64之间"))]
    pub vendor_code: Option<String>,
    #[validate(length(min = 1, max = 512, message = "上游API基础地址长度必须在1-512之间"))]
    pub base_url: Option<String>,
    pub models: Option<Vec<String>>,
    pub model_mapping: Option<serde_json::Map<String, serde_json::Value>>,
    pub channel_group: Option<String>,
    pub endpoint_scopes: Option<Vec<String>>,
    pub capabilities: Option<Vec<String>>,
    pub weight: Option<i32>,
    pub priority: Option<i32>,
    pub config: Option<serde_json::Map<String, serde_json::Value>>,
    pub auto_ban: Option<bool>,
    #[validate(length(max = 128, message = "测速模型名长度不能超过128"))]
    pub test_model: Option<String>,
    pub status: Option<ChannelStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateChannelDto {
    pub fn apply_to(self, active: &mut channel::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(name) = self.name {
            active.name = Set(name);
        }
        if let Some(channel_type) = self.channel_type {
            active.channel_type = Set(channel_type);
        }
        if let Some(vendor_code) = self.vendor_code {
            active.vendor_code = Set(vendor_code);
        }
        if let Some(base_url) = self.base_url {
            active.base_url = Set(base_url);
        }
        if let Some(models) = self.models
            && let Ok(v) = serde_json::to_value(models)
        {
            active.models = Set(v);
        }
        if let Some(model_mapping) = self.model_mapping {
            active.model_mapping = Set(serde_json::Value::Object(model_mapping));
        }
        if let Some(channel_group) = self.channel_group {
            active.channel_group = Set(channel_group);
        }
        if let Some(endpoint_scopes) = self.endpoint_scopes
            && let Ok(v) = serde_json::to_value(endpoint_scopes)
        {
            active.endpoint_scopes = Set(v);
        }
        if let Some(capabilities) = self.capabilities
            && let Ok(v) = serde_json::to_value(capabilities)
        {
            active.capabilities = Set(v);
        }
        if let Some(weight) = self.weight {
            active.weight = Set(weight);
        }
        if let Some(priority) = self.priority {
            active.priority = Set(priority);
        }
        if let Some(config) = self.config {
            active.config = Set(serde_json::Value::Object(config));
        }
        if let Some(auto_ban) = self.auto_ban {
            active.auto_ban = Set(auto_ban);
        }
        if let Some(test_model) = self.test_model {
            active.test_model = Set(test_model);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelQueryDto
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelQueryDto {
    pub keyword: Option<String>,
    pub status: Option<ChannelStatus>,
    pub channel_type: Option<ChannelType>,
    pub vendor_code: Option<String>,
    pub channel_group: Option<String>,
    pub id_sort: Option<bool>,
}

impl ChannelQueryDto {
    pub fn has_filters(&self) -> bool {
        self.keyword.is_some()
            || self.status.is_some()
            || self.channel_type.is_some()
            || self.vendor_code.is_some()
            || self.channel_group.is_some()
    }
}

impl From<ChannelQueryDto> for Condition {
    fn from(query: ChannelQueryDto) -> Self {
        let mut cond = Condition::all();
        // 默认排除已软删除的
        cond = cond.add(channel::Column::DeletedAt.is_null());
        if let Some(status) = query.status {
            cond = cond.add(channel::Column::Status.eq(status));
        }
        if let Some(channel_type) = query.channel_type {
            cond = cond.add(channel::Column::ChannelType.eq(channel_type));
        }
        if let Some(ref vendor_code) = query.vendor_code {
            cond = cond.add(channel::Column::VendorCode.eq(vendor_code.clone()));
        }
        if let Some(ref channel_group) = query.channel_group {
            cond = cond.add(channel::Column::ChannelGroup.eq(channel_group.clone()));
        }
        if let Some(ref keyword) = query.keyword {
            let keyword_cond = Condition::any()
                .add(channel::Column::Name.contains(keyword))
                .add(channel::Column::Remark.contains(keyword));
            cond = cond.add(keyword_cond);
        }
        cond
    }
}
