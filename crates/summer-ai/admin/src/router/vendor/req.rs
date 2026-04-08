use schemars::JsonSchema;
use sea_orm::{ActiveValue, ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use summer_ai_model::entity::vendor;

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VendorQuery {
    pub vendor_code: Option<String>,
    pub vendor_name: Option<String>,
    pub api_style: Option<String>,
    pub enabled: Option<bool>,
}

impl From<VendorQuery> for Condition {
    fn from(req: VendorQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(vendor_code) = req.vendor_code {
            condition = condition.add(vendor::Column::VendorCode.eq(vendor_code));
        }
        if let Some(vendor_name) = req.vendor_name {
            condition = condition.add(vendor::Column::VendorName.contains(&vendor_name));
        }
        if let Some(api_style) = req.api_style {
            condition = condition.add(vendor::Column::ApiStyle.eq(api_style));
        }
        if let Some(enabled) = req.enabled {
            condition = condition.add(vendor::Column::Enabled.eq(enabled));
        }
        condition
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateVendorReq {
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVendorReq {
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

fn default_true() -> bool {
    true
}

fn default_vendor_sort(vendor_code: &str) -> Option<i32> {
    match vendor_code.trim().to_ascii_lowercase().as_str() {
        "openai" => Some(1),
        "anthropic" => Some(2),
        "azure" => Some(3),
        "baidu" => Some(4),
        "ali" => Some(5),
        "google" => Some(6),
        "ollama" => Some(7),
        "deepseek" => Some(8),
        "groq" => Some(9),
        "openrouter" => Some(10),
        _ => None,
    }
}

impl CreateVendorReq {
    pub fn into_active_model(self, operator: &str) -> vendor::ActiveModel {
        let vendor_sort = if self.vendor_sort == 0 {
            default_vendor_sort(&self.vendor_code).unwrap_or(0)
        } else {
            self.vendor_sort
        };

        vendor::ActiveModel {
            vendor_code: Set(self.vendor_code),
            vendor_name: Set(self.vendor_name),
            api_style: Set(self.api_style),
            icon: Set(self.icon),
            description: Set(self.description),
            base_url: Set(self.base_url),
            doc_url: Set(self.doc_url),
            metadata: Set(self.metadata),
            vendor_sort: Set(vendor_sort),
            enabled: Set(self.enabled),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }
}

impl UpdateVendorReq {
    pub fn apply_to(self, active: &mut vendor::ActiveModel, operator: &str) {
        if let Some(vendor_name) = self.vendor_name {
            active.vendor_name = Set(vendor_name);
        }
        if let Some(api_style) = self.api_style {
            active.api_style = Set(api_style);
        }
        if let Some(icon) = self.icon {
            active.icon = Set(icon);
        }
        if let Some(description) = self.description {
            active.description = Set(description);
        }
        if let Some(base_url) = self.base_url {
            active.base_url = Set(base_url);
        }
        if let Some(doc_url) = self.doc_url {
            active.doc_url = Set(doc_url);
        }
        if let Some(metadata) = self.metadata {
            active.metadata = Set(metadata);
        }
        if let Some(vendor_sort) = self.vendor_sort {
            let vendor_sort = if vendor_sort == 0 {
                let vendor_code = match &active.vendor_code {
                    ActiveValue::Set(value) | ActiveValue::Unchanged(value) => value.clone(),
                    ActiveValue::NotSet => String::new(),
                };
                default_vendor_sort(&vendor_code).unwrap_or(0)
            } else {
                vendor_sort
            };
            active.vendor_sort = Set(vendor_sort);
        }
        if let Some(enabled) = self.enabled {
            active.enabled = Set(enabled);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
        active.update_by = Set(operator.to_string());
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue::Set;

    use super::{CreateVendorReq, UpdateVendorReq};
    use summer_ai_model::entity::vendor;

    fn create_vendor_req(vendor_code: &str, vendor_sort: i32) -> CreateVendorReq {
        CreateVendorReq {
            vendor_code: vendor_code.to_string(),
            vendor_name: vendor_code.to_string(),
            api_style: String::new(),
            icon: String::new(),
            description: String::new(),
            base_url: String::new(),
            doc_url: String::new(),
            metadata: serde_json::json!({}),
            vendor_sort,
            enabled: true,
            remark: String::new(),
        }
    }

    #[test]
    fn create_vendor_req_assigns_builtin_vendor_sort_when_zero() {
        let openai = create_vendor_req("openai", 0).into_active_model("admin");
        let anthropic = create_vendor_req("anthropic", 0).into_active_model("admin");
        let azure = create_vendor_req("azure", 0).into_active_model("admin");
        let google = create_vendor_req("google", 0).into_active_model("admin");
        let deepseek = create_vendor_req("deepseek", 0).into_active_model("admin");

        assert_eq!(openai.vendor_sort, Set(1));
        assert_eq!(anthropic.vendor_sort, Set(2));
        assert_eq!(azure.vendor_sort, Set(3));
        assert_eq!(google.vendor_sort, Set(6));
        assert_eq!(deepseek.vendor_sort, Set(8));
    }

    #[test]
    fn create_vendor_req_preserves_explicit_vendor_sort() {
        let vendor = create_vendor_req("azure", 99).into_active_model("admin");
        assert_eq!(vendor.vendor_sort, Set(99));
    }

    #[test]
    fn update_vendor_req_assigns_builtin_vendor_sort_when_zero() {
        let mut active = vendor::ActiveModel {
            vendor_code: Set("azure".to_string()),
            vendor_sort: Set(20),
            ..Default::default()
        };

        UpdateVendorReq {
            vendor_name: None,
            api_style: None,
            icon: None,
            description: None,
            base_url: None,
            doc_url: None,
            metadata: None,
            vendor_sort: Some(0),
            enabled: None,
            remark: None,
        }
        .apply_to(&mut active, "admin");

        assert_eq!(active.vendor_sort, Set(3));
    }
}
