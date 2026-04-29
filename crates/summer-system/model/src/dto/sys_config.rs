//! 系统参数配置 DTO

use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::{sys_config, sys_config_group};

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfigDto {
    #[validate(length(min = 1, max = 100, message = "配置名称长度必须在1-100之间"))]
    pub config_name: String,
    #[validate(length(min = 1, max = 100, message = "配置键长度必须在1-100之间"))]
    pub config_key: String,
    pub config_value: Option<String>,
    pub default_value: Option<String>,
    pub value_type: Option<sys_config::ValueType>,
    #[validate(range(min = 1, message = "配置分组ID必须大于0"))]
    pub config_group_id: i64,
    #[validate(length(max = 100, message = "候选项字典类型编码长度不能超过100"))]
    pub option_dict_type: Option<String>,
    pub config_sort: Option<i32>,
    pub enabled: Option<bool>,
    pub is_system: Option<bool>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl From<CreateConfigDto> for sys_config::ActiveModel {
    fn from(dto: CreateConfigDto) -> Self {
        Self {
            id: NotSet,
            config_name: Set(dto.config_name),
            config_key: Set(dto.config_key),
            config_value: Set(dto.config_value.unwrap_or_default()),
            default_value: Set(dto.default_value.unwrap_or_default()),
            value_type: Set(dto.value_type.unwrap_or(sys_config::ValueType::Text)),
            config_group_id: Set(dto.config_group_id),
            option_dict_type: Set(dto.option_dict_type.unwrap_or_default()),
            config_sort: Set(dto.config_sort.unwrap_or(0)),
            enabled: Set(dto.enabled.unwrap_or(true)),
            is_system: Set(dto.is_system.unwrap_or(false)),
            remark: Set(dto.remark.unwrap_or_default()),
            create_by: NotSet,
            create_time: NotSet,
            update_by: NotSet,
            update_time: NotSet,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigDto {
    #[validate(length(min = 1, max = 100, message = "配置名称长度必须在1-100之间"))]
    pub config_name: Option<String>,
    #[validate(length(min = 1, max = 100, message = "配置键长度必须在1-100之间"))]
    pub config_key: Option<String>,
    pub config_value: Option<String>,
    pub default_value: Option<String>,
    pub value_type: Option<sys_config::ValueType>,
    #[validate(range(min = 1, message = "配置分组ID必须大于0"))]
    pub config_group_id: Option<i64>,
    #[validate(length(max = 100, message = "候选项字典类型编码长度不能超过100"))]
    pub option_dict_type: Option<String>,
    pub config_sort: Option<i32>,
    pub enabled: Option<bool>,
    pub is_system: Option<bool>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateConfigDto {
    pub fn apply_to(self, active: &mut sys_config::ActiveModel) {
        if let Some(config_name) = self.config_name {
            active.config_name = Set(config_name);
        }
        if let Some(config_key) = self.config_key {
            active.config_key = Set(config_key);
        }
        if let Some(config_value) = self.config_value {
            active.config_value = Set(config_value);
        }
        if let Some(default_value) = self.default_value {
            active.default_value = Set(default_value);
        }
        if let Some(value_type) = self.value_type {
            active.value_type = Set(value_type);
        }
        if let Some(config_group_id) = self.config_group_id {
            active.config_group_id = Set(config_group_id);
        }
        if let Some(option_dict_type) = self.option_dict_type {
            active.option_dict_type = Set(option_dict_type);
        }
        if let Some(config_sort) = self.config_sort {
            active.config_sort = Set(config_sort);
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
pub struct ConfigQueryDto {
    pub id: Option<i64>,
    pub config_name: Option<String>,
    pub config_key: Option<String>,
    pub value_type: Option<sys_config::ValueType>,
    pub option_dict_type: Option<String>,
    pub enabled: Option<bool>,
    pub is_system: Option<bool>,
    pub create_time_start: Option<chrono::NaiveDateTime>,
    pub create_time_end: Option<chrono::NaiveDateTime>,
    pub update_time_start: Option<chrono::NaiveDateTime>,
    pub update_time_end: Option<chrono::NaiveDateTime>,
}

impl ConfigQueryDto {
    pub fn has_filters(&self) -> bool {
        self.id.is_some()
            || self.config_name.is_some()
            || self.config_key.is_some()
            || self.value_type.is_some()
            || self.option_dict_type.is_some()
            || self.enabled.is_some()
            || self.is_system.is_some()
            || self.create_time_start.is_some()
            || self.create_time_end.is_some()
            || self.update_time_start.is_some()
            || self.update_time_end.is_some()
    }
}

impl From<ConfigQueryDto> for Condition {
    fn from(query: ConfigQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(id) = query.id {
            cond = cond.add(sys_config::Column::Id.eq(id));
        }
        if let Some(config_name) = query.config_name {
            cond = cond.add(sys_config::Column::ConfigName.contains(config_name));
        }
        if let Some(config_key) = query.config_key {
            cond = cond.add(sys_config::Column::ConfigKey.contains(config_key));
        }
        if let Some(value_type) = query.value_type {
            cond = cond.add(sys_config::Column::ValueType.eq(value_type));
        }
        if let Some(option_dict_type) = query.option_dict_type {
            cond = cond.add(sys_config::Column::OptionDictType.contains(option_dict_type));
        }
        if let Some(enabled) = query.enabled {
            cond = cond.add(sys_config::Column::Enabled.eq(enabled));
        }
        if let Some(is_system) = query.is_system {
            cond = cond.add(sys_config::Column::IsSystem.eq(is_system));
        }
        if let Some(start) = query.create_time_start {
            cond = cond.add(sys_config::Column::CreateTime.gte(start));
        }
        if let Some(end) = query.create_time_end {
            cond = cond.add(sys_config::Column::CreateTime.lte(end));
        }
        if let Some(start) = query.update_time_start {
            cond = cond.add(sys_config::Column::UpdateTime.gte(start));
        }
        if let Some(end) = query.update_time_end {
            cond = cond.add(sys_config::Column::UpdateTime.lte(end));
        }
        cond
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigGroupFilterQueryDto {
    pub config_group_id: Option<i64>,
    pub config_group_name: Option<String>,
    pub config_group_code: Option<String>,
}

impl From<ConfigGroupFilterQueryDto> for Condition {
    fn from(query: ConfigGroupFilterQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(config_group_id) = query.config_group_id {
            cond = cond.add(sys_config_group::Column::Id.eq(config_group_id));
        }
        if let Some(config_group_name) = query.config_group_name {
            cond = cond.add(sys_config_group::Column::GroupName.contains(config_group_name));
        }
        if let Some(config_group_code) = query.config_group_code {
            cond = cond.add(sys_config_group::Column::GroupCode.contains(config_group_code));
        }
        cond
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ConfigKeysDto {
    #[validate(length(min = 1, max = 100, message = "配置键列表数量必须在1-100之间"))]
    pub config_keys: Vec<String>,
}
