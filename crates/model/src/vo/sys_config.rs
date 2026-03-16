//! Generated admin VO skeleton.

use common::serde_utils::datetime_format;
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::sys_config;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigVo {
    /// 配置ID
    pub id: i64,
    /// 配置名称
    pub config_name: String,
    /// 配置键
    pub config_key: String,
    /// 当前配置值
    pub config_value: String,
    /// 默认配置值
    pub default_value: String,
    /// 值类型
    pub value_type: i16,
    /// 配置分组编码
    pub config_group: String,
    /// 候选项字典类型编码
    pub option_dict_type: String,
    /// 同分组内排序
    pub config_sort: i32,
    /// 是否启用
    pub enabled: bool,
    /// 是否系统内置
    pub is_system: bool,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: chrono::NaiveDateTime,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: chrono::NaiveDateTime,
}

impl From<sys_config::Model> for ConfigVo {
    fn from(model: sys_config::Model) -> Self {
        Self {
            id: model.id,
            config_name: model.config_name,
            config_key: model.config_key,
            config_value: model.config_value,
            default_value: model.default_value,
            value_type: model.value_type,
            config_group: model.config_group,
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
