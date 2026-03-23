//! 系统参数配置 VO

use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Serialize;
use summer_common::serde_utils::datetime_format;

use crate::entity::{sys_config, sys_config_group};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigValueVo {
    pub config_name: String,
    pub config_key: String,
    pub config_value: String,
    pub default_value: String,
    pub value_type: sys_config::ValueType,
    pub option_dict_type: String,
}

impl From<sys_config::Model> for ConfigValueVo {
    fn from(model: sys_config::Model) -> Self {
        Self {
            config_name: model.config_name,
            config_key: model.config_key,
            config_value: model.config_value,
            default_value: model.default_value,
            value_type: model.value_type,
            option_dict_type: model.option_dict_type,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDetailVo {
    pub id: i64,
    pub config_name: String,
    pub config_key: String,
    pub config_value: String,
    pub default_value: String,
    pub value_type: sys_config::ValueType,
    pub config_group_id: i64,
    pub config_group_name: String,
    pub config_group_code: String,
    pub option_dict_type: String,
    pub config_sort: i32,
    pub enabled: bool,
    pub is_system: bool,
    pub remark: String,
    pub create_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    pub update_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl ConfigDetailVo {
    pub fn from_model(model: sys_config::Model, group: Option<sys_config_group::Model>) -> Self {
        let (config_group_name, config_group_code) = group
            .map(|group| (group.group_name, group.group_code))
            .unwrap_or_default();

        Self {
            id: model.id,
            config_name: model.config_name,
            config_key: model.config_key,
            config_value: model.config_value,
            default_value: model.default_value,
            value_type: model.value_type,
            config_group_id: model.config_group_id,
            config_group_name,
            config_group_code,
            option_dict_type: model.option_dict_type,
            config_sort: model.config_sort,
            enabled: model.enabled,
            is_system: model.is_system,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigGroupItemVo {
    pub id: i64,
    pub config_name: String,
    pub config_key: String,
    pub config_value: String,
    pub default_value: String,
    pub value_type: sys_config::ValueType,
    pub option_dict_type: String,
    pub config_sort: i32,
    pub enabled: bool,
    pub is_system: bool,
    pub remark: String,
}

impl From<sys_config::Model> for ConfigGroupItemVo {
    fn from(model: sys_config::Model) -> Self {
        Self {
            id: model.id,
            config_name: model.config_name,
            config_key: model.config_key,
            config_value: model.config_value,
            default_value: model.default_value,
            value_type: model.value_type,
            option_dict_type: model.option_dict_type,
            config_sort: model.config_sort,
            enabled: model.enabled,
            is_system: model.is_system,
            remark: model.remark,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigGroupBlockVo {
    pub group_id: i64,
    pub group_name: String,
    pub group_code: String,
    pub group_sort: i32,
    pub items: Vec<ConfigGroupItemVo>,
}

impl ConfigGroupBlockVo {
    pub fn from_model(group: sys_config_group::Model, items: Vec<ConfigGroupItemVo>) -> Self {
        Self {
            group_id: group.id,
            group_name: group.group_name,
            group_code: group.group_code,
            group_sort: group.group_sort,
            items,
        }
    }
}
