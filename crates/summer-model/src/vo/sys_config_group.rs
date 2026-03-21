//! 系统参数分组 VO

use chrono::NaiveDateTime;
use summer_common::serde_utils::datetime_format;
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::sys_config_group;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigGroupVo {
    pub id: i64,
    pub group_name: String,
    pub group_code: String,
    pub group_sort: i32,
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

impl From<sys_config_group::Model> for ConfigGroupVo {
    fn from(model: sys_config_group::Model) -> Self {
        Self {
            id: model.id,
            group_name: model.group_name,
            group_code: model.group_code,
            group_sort: model.group_sort,
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
