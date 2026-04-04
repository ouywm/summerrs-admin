use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::vendor;

/// 创建供应商
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateVendorDto {
    #[validate(length(min = 1, max = 64))]
    pub vendor_code: String,
    #[validate(length(min = 1, max = 128))]
    pub vendor_name: String,
    #[serde(default)]
    pub api_style: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub doc_url: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub vendor_sort: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub remark: String,
}

fn default_true() -> bool {
    true
}

impl CreateVendorDto {
    pub fn into_active_model(self, operator: &str) -> vendor::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        vendor::ActiveModel {
            vendor_code: Set(self.vendor_code),
            vendor_name: Set(self.vendor_name),
            api_style: Set(self.api_style),
            icon: Set(self.icon),
            description: Set(self.description),
            base_url: Set(self.base_url),
            doc_url: Set(self.doc_url),
            metadata: Set(self.metadata),
            vendor_sort: Set(self.vendor_sort),
            enabled: Set(self.enabled),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

/// 更新供应商
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVendorDto {
    #[validate(length(min = 1, max = 128))]
    pub vendor_name: Option<String>,
    pub api_style: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub base_url: Option<String>,
    pub doc_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub vendor_sort: Option<i32>,
    pub enabled: Option<bool>,
    pub remark: Option<String>,
}

impl UpdateVendorDto {
    pub fn apply_to(self, active: &mut vendor::ActiveModel, operator: &str) {
        if let Some(v) = self.vendor_name {
            active.vendor_name = Set(v);
        }
        if let Some(v) = self.api_style {
            active.api_style = Set(v);
        }
        if let Some(v) = self.icon {
            active.icon = Set(v);
        }
        if let Some(v) = self.description {
            active.description = Set(v);
        }
        if let Some(v) = self.base_url {
            active.base_url = Set(v);
        }
        if let Some(v) = self.doc_url {
            active.doc_url = Set(v);
        }
        if let Some(v) = self.metadata {
            active.metadata = Set(v);
        }
        if let Some(v) = self.vendor_sort {
            active.vendor_sort = Set(v);
        }
        if let Some(v) = self.enabled {
            active.enabled = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}

/// 查询供应商
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryVendorDto {
    pub vendor_code: Option<String>,
    pub vendor_name: Option<String>,
    pub api_style: Option<String>,
    pub enabled: Option<bool>,
}

impl From<QueryVendorDto> for sea_orm::Condition {
    fn from(dto: QueryVendorDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.vendor_code {
            cond = cond.add(vendor::Column::VendorCode.eq(v));
        }
        if let Some(v) = dto.vendor_name {
            cond = cond.add(vendor::Column::VendorName.contains(&v));
        }
        if let Some(v) = dto.api_style {
            cond = cond.add(vendor::Column::ApiStyle.eq(v));
        }
        if let Some(v) = dto.enabled {
            cond = cond.add(vendor::Column::Enabled.eq(v));
        }
        cond
    }
}
