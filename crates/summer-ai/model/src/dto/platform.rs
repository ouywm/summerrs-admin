use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::config_entry;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfigEntryDto {
    pub config_key: String,
    pub config_value: String,
    #[serde(default = "default_string")]
    pub value_type: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
}
fn default_string() -> String {
    "string".into()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigEntryDto {
    pub config_value: Option<String>,
    pub description: Option<String>,
    pub status: Option<i16>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryConfigEntryDto {
    pub category: Option<String>,
    pub config_key: Option<String>,
    pub status: Option<i16>,
}

impl From<QueryConfigEntryDto> for sea_orm::Condition {
    fn from(dto: QueryConfigEntryDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut c = sea_orm::Condition::all();
        if let Some(v) = dto.category {
            c = c.add(config_entry::Column::Category.eq(v));
        }
        if let Some(v) = dto.config_key {
            c = c.add(config_entry::Column::ConfigKey.contains(&v));
        }
        if let Some(v) = dto.status {
            c = c.add(config_entry::Column::Status.eq(v));
        }
        c
    }
}
