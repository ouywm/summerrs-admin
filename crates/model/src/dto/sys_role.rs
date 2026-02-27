use chrono::NaiveDate;
use common::request::PageQuery;
use schemars::JsonSchema;
use sea_orm::Set;
use serde::Deserialize;
use validator::Validate;

use crate::entity::sys_role;

#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoleDto {
    #[validate(length(min = 1, max = 64, message = "角色名称长度必须在1-64之间"))]
    pub role_name: String,
    #[validate(length(min = 1, max = 32, message = "角色编码长度必须在1-32之间"))]
    pub role_code: String,
    #[validate(length(max = 512, message = "描述长度不能超过512"))]
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

impl From<CreateRoleDto> for sys_role::ActiveModel {
    fn from(dto: CreateRoleDto) -> Self {
        Self {
            role_name: Set(dto.role_name),
            role_code: Set(dto.role_code),
            description: Set(dto.description.unwrap_or_default()),
            enabled: Set(dto.enabled.unwrap_or(true)),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoleDto {
    #[validate(length(min = 1, max = 64, message = "角色名称长度必须在1-64之间"))]
    pub role_name: Option<String>,
    #[validate(length(max = 512, message = "描述长度不能超过512"))]
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

impl UpdateRoleDto {
    /// 将 DTO 中的非空字段应用到 ActiveModel
    pub fn apply_to(self, active: &mut sys_role::ActiveModel) {
        if let Some(role_name) = self.role_name {
            active.role_name = Set(role_name);
        }
        if let Some(description) = self.description {
            active.description = Set(description);
        }
        if let Some(enabled) = self.enabled {
            active.enabled = Set(enabled);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleQueryDto {
    #[serde(flatten)]
    pub page: PageQuery,
    pub role_name: Option<String>,
    pub role_code: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub start_time: Option<NaiveDate>,
    pub end_time: Option<NaiveDate>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RolePermissionDto {
    pub menu_ids: Vec<i64>,
}
