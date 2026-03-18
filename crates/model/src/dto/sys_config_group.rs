//! 系统参数分组 DTO

use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sys_config_group;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfigGroupDto {
    #[validate(length(min = 1, max = 100, message = "分组名称长度必须在1-100之间"))]
    pub group_name: String,
    #[validate(length(min = 1, max = 64, message = "分组编码长度必须在1-64之间"))]
    pub group_code: String,
    pub group_sort: Option<i32>,
    pub enabled: Option<bool>,
    pub is_system: Option<bool>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl From<CreateConfigGroupDto> for sys_config_group::ActiveModel {
    fn from(dto: CreateConfigGroupDto) -> Self {
        Self {
            group_name: Set(dto.group_name),
            group_code: Set(dto.group_code),
            group_sort: Set(dto.group_sort.unwrap_or(0)),
            enabled: Set(dto.enabled.unwrap_or(true)),
            is_system: Set(dto.is_system.unwrap_or(false)),
            remark: Set(dto.remark.unwrap_or_default()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigGroupDto {
    #[validate(length(min = 1, max = 100, message = "分组名称长度必须在1-100之间"))]
    pub group_name: Option<String>,
    pub group_sort: Option<i32>,
    pub enabled: Option<bool>,
    pub is_system: Option<bool>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateConfigGroupDto {
    pub fn apply_to(self, active: &mut sys_config_group::ActiveModel) {
        if let Some(group_name) = self.group_name {
            active.group_name = Set(group_name);
        }
        if let Some(group_sort) = self.group_sort {
            active.group_sort = Set(group_sort);
        }
        if let Some(enabled) = self.enabled {
            active.enabled = Set(enabled);
        }
        if let Some(is_system) = self.is_system {
            active.is_system = Set(is_system);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigGroupQueryDto {
    pub id: Option<i64>,
    pub group_name: Option<String>,
    pub group_code: Option<String>,
    pub enabled: Option<bool>,
    pub is_system: Option<bool>,
}

impl From<ConfigGroupQueryDto> for Condition {
    fn from(query: ConfigGroupQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(id) = query.id {
            cond = cond.add(sys_config_group::Column::Id.eq(id));
        }
        if let Some(group_name) = query.group_name {
            cond = cond.add(sys_config_group::Column::GroupName.contains(group_name));
        }
        if let Some(group_code) = query.group_code {
            cond = cond.add(sys_config_group::Column::GroupCode.contains(group_code));
        }
        if let Some(enabled) = query.enabled {
            cond = cond.add(sys_config_group::Column::Enabled.eq(enabled));
        }
        if let Some(is_system) = query.is_system {
            cond = cond.add(sys_config_group::Column::IsSystem.eq(is_system));
        }
        cond
    }
}
