use chrono::NaiveDate;
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sys_role;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
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

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
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
    pub role_name: Option<String>,
    pub role_code: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub start_time: Option<NaiveDate>,
    pub end_time: Option<NaiveDate>,
}

impl From<RoleQueryDto> for Condition {
    fn from(query: RoleQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(name) = query.role_name {
            cond = cond.add(sys_role::Column::RoleName.contains(name));
        }
        if let Some(code) = query.role_code {
            cond = cond.add(sys_role::Column::RoleCode.contains(code));
        }
        if let Some(desc) = query.description {
            cond = cond.add(sys_role::Column::Description.contains(desc));
        }
        if let Some(enabled) = query.enabled {
            cond = cond.add(sys_role::Column::Enabled.eq(enabled));
        }
        if let Some(start) = query.start_time {
            cond = cond.add(sys_role::Column::CreateTime.gte(start));
        }
        if let Some(end) = query.end_time {
            cond = cond.add(sys_role::Column::CreateTime.lte(end));
        }
        cond
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RolePermissionDto {
    pub menu_ids: Vec<i64>,
}
