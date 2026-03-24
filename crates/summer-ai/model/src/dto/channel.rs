use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::channel::{self, ChannelStatus, ChannelType};

/// 创建渠道
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelDto {
    #[validate(length(min = 1, max = 128, message = "渠道名称长度 1-128"))]
    pub name: String,
    pub channel_type: ChannelType,
    #[validate(length(max = 64))]
    pub vendor_code: String,
    #[validate(url(message = "无效的 URL 格式"))]
    pub base_url: String,
    pub models: serde_json::Value,
    #[serde(default)]
    pub model_mapping: serde_json::Value,
    #[validate(length(min = 1, max = 64))]
    pub channel_group: String,
    #[serde(default)]
    pub endpoint_scopes: serde_json::Value,
    #[serde(default)]
    pub capabilities: serde_json::Value,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default = "default_true")]
    pub auto_ban: bool,
    #[serde(default)]
    pub test_model: String,
    #[serde(default)]
    pub remark: String,
}

fn default_weight() -> i32 {
    1
}
fn default_true() -> bool {
    true
}

impl CreateChannelDto {
    pub fn into_active_model(self, operator: &str) -> channel::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        channel::ActiveModel {
            name: Set(self.name),
            channel_type: Set(self.channel_type),
            vendor_code: Set(self.vendor_code),
            base_url: Set(self.base_url),
            status: Set(ChannelStatus::Enabled),
            models: Set(self.models),
            model_mapping: Set(self.model_mapping),
            channel_group: Set(self.channel_group),
            endpoint_scopes: Set(self.endpoint_scopes),
            capabilities: Set(self.capabilities),
            weight: Set(self.weight),
            priority: Set(self.priority),
            config: Set(self.config),
            auto_ban: Set(self.auto_ban),
            test_model: Set(self.test_model),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

/// 更新渠道
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelDto {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    pub channel_type: Option<ChannelType>,
    pub vendor_code: Option<String>,
    pub base_url: Option<String>,
    pub status: Option<ChannelStatus>,
    pub models: Option<serde_json::Value>,
    pub model_mapping: Option<serde_json::Value>,
    pub channel_group: Option<String>,
    pub endpoint_scopes: Option<serde_json::Value>,
    pub capabilities: Option<serde_json::Value>,
    pub weight: Option<i32>,
    pub priority: Option<i32>,
    pub config: Option<serde_json::Value>,
    pub auto_ban: Option<bool>,
    pub test_model: Option<String>,
    pub remark: Option<String>,
}

impl UpdateChannelDto {
    pub fn apply_to(self, active: &mut channel::ActiveModel, operator: &str) {
        if let Some(v) = self.name {
            active.name = Set(v);
        }
        if let Some(v) = self.channel_type {
            active.channel_type = Set(v);
        }
        if let Some(v) = self.vendor_code {
            active.vendor_code = Set(v);
        }
        if let Some(v) = self.base_url {
            active.base_url = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        if let Some(v) = self.models {
            active.models = Set(v);
        }
        if let Some(v) = self.model_mapping {
            active.model_mapping = Set(v);
        }
        if let Some(v) = self.channel_group {
            active.channel_group = Set(v);
        }
        if let Some(v) = self.endpoint_scopes {
            active.endpoint_scopes = Set(v);
        }
        if let Some(v) = self.capabilities {
            active.capabilities = Set(v);
        }
        if let Some(v) = self.weight {
            active.weight = Set(v);
        }
        if let Some(v) = self.priority {
            active.priority = Set(v);
        }
        if let Some(v) = self.config {
            active.config = Set(v);
        }
        if let Some(v) = self.auto_ban {
            active.auto_ban = Set(v);
        }
        if let Some(v) = self.test_model {
            active.test_model = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}

/// 查询渠道
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryChannelDto {
    pub name: Option<String>,
    pub status: Option<ChannelStatus>,
    pub channel_type: Option<ChannelType>,
    pub channel_group: Option<String>,
}

impl From<QueryChannelDto> for sea_orm::Condition {
    fn from(dto: QueryChannelDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all().add(channel::Column::DeletedAt.is_null());
        if let Some(name) = dto.name {
            cond = cond.add(channel::Column::Name.contains(&name));
        }
        if let Some(status) = dto.status {
            cond = cond.add(channel::Column::Status.eq(status));
        }
        if let Some(ct) = dto.channel_type {
            cond = cond.add(channel::Column::ChannelType.eq(ct));
        }
        if let Some(group) = dto.channel_group {
            cond = cond.add(channel::Column::ChannelGroup.eq(group));
        }
        cond
    }
}
