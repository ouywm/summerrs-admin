//! Generated admin DTO skeleton.

use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::sys_config;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfigDto {
    /// 配置名称
    pub config_name: String,
    /// 配置键
    pub config_key: String,
    /// 当前配置值
    pub config_value: Option<String>,
    /// 默认配置值
    pub default_value: Option<String>,
    /// 值类型
    pub value_type: Option<i16>,
    /// 配置分组编码
    pub config_group: Option<String>,
    /// 候选项字典类型编码
    pub option_dict_type: Option<String>,
    /// 同分组内排序
    pub config_sort: Option<i32>,
    /// 是否启用
    pub enabled: Option<bool>,
    /// 是否系统内置
    pub is_system: Option<bool>,
    /// 备注
    pub remark: Option<String>,
}

impl From<CreateConfigDto> for sys_config::ActiveModel {
    fn from(dto: CreateConfigDto) -> Self {
        Self {
            config_name: Set(dto.config_name),
            config_key: Set(dto.config_key),
            config_value: dto.config_value.map(Set).unwrap_or_default(),
            default_value: dto.default_value.map(Set).unwrap_or_default(),
            value_type: dto.value_type.map(Set).unwrap_or_default(),
            config_group: dto.config_group.map(Set).unwrap_or_default(),
            option_dict_type: dto.option_dict_type.map(Set).unwrap_or_default(),
            config_sort: dto.config_sort.map(Set).unwrap_or_default(),
            enabled: dto.enabled.map(Set).unwrap_or_default(),
            is_system: dto.is_system.map(Set).unwrap_or_default(),
            remark: dto.remark.map(Set).unwrap_or_default(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigDto {
    /// 配置名称
    pub config_name: Option<String>,
    /// 配置键
    pub config_key: Option<String>,
    /// 当前配置值
    pub config_value: Option<String>,
    /// 默认配置值
    pub default_value: Option<String>,
    /// 值类型
    pub value_type: Option<i16>,
    /// 配置分组编码
    pub config_group: Option<String>,
    /// 候选项字典类型编码
    pub option_dict_type: Option<String>,
    /// 同分组内排序
    pub config_sort: Option<i32>,
    /// 是否启用
    pub enabled: Option<bool>,
    /// 是否系统内置
    pub is_system: Option<bool>,
    /// 备注
    pub remark: Option<String>,
}

impl UpdateConfigDto {
    pub fn apply_to(self, active: &mut sys_config::ActiveModel) {
        if let Some(value) = self.config_name {
            active.config_name = Set(value);
        }
        if let Some(value) = self.config_key {
            active.config_key = Set(value);
        }
        if let Some(value) = self.config_value {
            active.config_value = Set(value);
        }
        if let Some(value) = self.default_value {
            active.default_value = Set(value);
        }
        if let Some(value) = self.value_type {
            active.value_type = Set(value);
        }
        if let Some(value) = self.config_group {
            active.config_group = Set(value);
        }
        if let Some(value) = self.option_dict_type {
            active.option_dict_type = Set(value);
        }
        if let Some(value) = self.config_sort {
            active.config_sort = Set(value);
        }
        if let Some(value) = self.enabled {
            active.enabled = Set(value);
        }
        if let Some(value) = self.is_system {
            active.is_system = Set(value);
        }
        if let Some(value) = self.remark {
            active.remark = Set(value);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigQueryDto {
    /// 配置ID
    pub id: Option<i64>,
    /// 配置名称
    pub config_name: Option<String>,
    /// 配置键
    pub config_key: Option<String>,
    /// 当前配置值
    pub config_value: Option<String>,
    /// 默认配置值
    pub default_value: Option<String>,
    /// 值类型
    pub value_type: Option<i16>,
    /// 配置分组编码
    pub config_group: Option<String>,
    /// 候选项字典类型编码
    pub option_dict_type: Option<String>,
    /// 同分组内排序
    pub config_sort: Option<i32>,
    /// 是否启用
    pub enabled: Option<bool>,
    /// 是否系统内置
    pub is_system: Option<bool>,
    /// 备注
    pub remark: Option<String>,
    /// 创建人
    pub create_by: Option<String>,
    /// 创建时间
    pub create_time: Option<chrono::NaiveDateTime>,
    /// 创建时间开始
    pub create_time_start: Option<chrono::NaiveDateTime>,
    /// 创建时间结束
    pub create_time_end: Option<chrono::NaiveDateTime>,
    /// 更新人
    pub update_by: Option<String>,
    /// 更新时间
    pub update_time: Option<chrono::NaiveDateTime>,
    /// 更新时间开始
    pub update_time_start: Option<chrono::NaiveDateTime>,
    /// 更新时间结束
    pub update_time_end: Option<chrono::NaiveDateTime>,
}

impl From<ConfigQueryDto> for Condition {
    fn from(query: ConfigQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(value) = query.id {
            cond = cond.add(sys_config::Column::Id.eq(value));
        }
        if let Some(value) = query.config_name {
            cond = cond.add(sys_config::Column::ConfigName.contains(value));
        }
        if let Some(value) = query.config_key {
            cond = cond.add(sys_config::Column::ConfigKey.contains(value));
        }
        if let Some(value) = query.config_value {
            cond = cond.add(sys_config::Column::ConfigValue.contains(value));
        }
        if let Some(value) = query.default_value {
            cond = cond.add(sys_config::Column::DefaultValue.contains(value));
        }
        if let Some(value) = query.value_type {
            cond = cond.add(sys_config::Column::ValueType.eq(value));
        }
        if let Some(value) = query.config_group {
            cond = cond.add(sys_config::Column::ConfigGroup.contains(value));
        }
        if let Some(value) = query.option_dict_type {
            cond = cond.add(sys_config::Column::OptionDictType.contains(value));
        }
        if let Some(value) = query.config_sort {
            cond = cond.add(sys_config::Column::ConfigSort.eq(value));
        }
        if let Some(value) = query.enabled {
            cond = cond.add(sys_config::Column::Enabled.eq(value));
        }
        if let Some(value) = query.is_system {
            cond = cond.add(sys_config::Column::IsSystem.eq(value));
        }
        if let Some(value) = query.remark {
            cond = cond.add(sys_config::Column::Remark.contains(value));
        }
        if let Some(value) = query.create_by {
            cond = cond.add(sys_config::Column::CreateBy.contains(value));
        }
        if let Some(value) = query.create_time {
            cond = cond.add(sys_config::Column::CreateTime.eq(value));
        }
        if let Some(start) = query.create_time_start {
            cond = cond.add(sys_config::Column::CreateTime.gte(start));
        }
        if let Some(end) = query.create_time_end {
            cond = cond.add(sys_config::Column::CreateTime.lte(end));
        }
        if let Some(value) = query.update_by {
            cond = cond.add(sys_config::Column::UpdateBy.contains(value));
        }
        if let Some(value) = query.update_time {
            cond = cond.add(sys_config::Column::UpdateTime.eq(value));
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
