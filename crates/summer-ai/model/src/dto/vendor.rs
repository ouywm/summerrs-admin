use crate::entity::routing::vendor::{self, ApiStyle};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateVendorDto {
    #[validate(length(min = 1, max = 64, message = "供应商编码长度必须在1-64之间"))]
    pub vendor_code: String,
    #[validate(length(min = 1, max = 128, message = "供应商名称长度必须在1-128之间"))]
    pub vendor_name: String,
    pub api_style: ApiStyle,
    #[validate(length(max = 512, message = "图标地址长度不能超过512"))]
    pub icon: Option<String>,
    #[validate(length(max = 4000, message = "供应商简介长度不能超过4000"))]
    pub description: Option<String>,
    #[validate(length(max = 512, message = "基础地址长度不能超过512"))]
    pub base_url: Option<String>,
    #[validate(length(max = 512, message = "文档地址长度不能超过512"))]
    pub doc_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub vendor_sort: Option<i32>,
    pub enabled: Option<bool>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateVendorDto {
    pub fn into_active_model(self, operator: &str) -> vendor::ActiveModel {
        vendor::ActiveModel {
            id: NotSet,
            vendor_code: Set(self.vendor_code),
            vendor_name: Set(self.vendor_name),
            api_style: Set(self.api_style),
            icon: Set(self.icon.unwrap_or_default()),
            description: Set(self.description.unwrap_or_default()),
            base_url: Set(self.base_url.unwrap_or_default()),
            doc_url: Set(self.doc_url.unwrap_or_default()),
            metadata: Set(self.metadata.unwrap_or_else(|| serde_json::json!({}))),
            vendor_sort: Set(self.vendor_sort.unwrap_or(0)),
            enabled: Set(self.enabled.unwrap_or(true)),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            create_time: NotSet,
            update_by: Set(operator.to_string()),
            update_time: NotSet,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVendorDto {
    #[validate(length(min = 1, max = 128, message = "供应商名称长度必须在1-128之间"))]
    pub vendor_name: Option<String>,
    pub api_style: Option<ApiStyle>,
    #[validate(length(max = 512, message = "图标地址长度不能超过512"))]
    pub icon: Option<String>,
    #[validate(length(max = 4000, message = "供应商简介长度不能超过4000"))]
    pub description: Option<String>,
    #[validate(length(max = 512, message = "基础地址长度不能超过512"))]
    pub base_url: Option<String>,
    #[validate(length(max = 512, message = "文档地址长度不能超过512"))]
    pub doc_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub vendor_sort: Option<i32>,
    pub enabled: Option<bool>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateVendorDto {
    pub fn apply_to(self, active: &mut vendor::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
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
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VendorQueryDto {
    pub vendor_code: Option<String>,
    pub api_style: Option<ApiStyle>,
    pub enabled: Option<bool>,
    pub keyword: Option<String>,
}

impl From<VendorQueryDto> for Condition {
    fn from(query: VendorQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.vendor_code {
            cond = cond.add(vendor::Column::VendorCode.eq(v));
        }
        if let Some(v) = query.api_style {
            cond = cond.add(vendor::Column::ApiStyle.eq(v));
        }
        if let Some(v) = query.enabled {
            cond = cond.add(vendor::Column::Enabled.eq(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(vendor::Column::VendorCode.contains(&keyword))
                        .add(vendor::Column::VendorName.contains(&keyword))
                        .add(vendor::Column::Description.contains(&keyword))
                        .add(vendor::Column::Remark.contains(&keyword)),
                );
            }
        }
        cond
    }
}
