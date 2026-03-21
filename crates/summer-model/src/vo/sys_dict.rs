use chrono::NaiveDateTime;
use summer_common::serde_utils::datetime_format;
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::{sys_dict_data, sys_dict_type};

// ============================================================
// 字典类型 VO
// ============================================================

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictTypeVo {
    pub id: i64,
    pub dict_name: String,
    pub dict_type: String,
    pub status: sys_dict_type::DictStatus,
    pub is_system: bool,
    pub remark: String,
    pub create_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    pub update_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl From<sys_dict_type::Model> for DictTypeVo {
    fn from(model: sys_dict_type::Model) -> Self {
        Self {
            id: model.id,
            dict_name: model.dict_name,
            dict_type: model.dict_type,
            status: model.status,
            is_system: model.is_system,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

// ============================================================
// 字典数据 VO
// ============================================================

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictDataVo {
    pub id: i64,
    pub dict_type: String,
    pub dict_label: String,
    pub dict_value: String,
    pub dict_sort: i32,
    pub css_class: String,
    pub list_class: String,
    pub is_default: bool,
    pub status: sys_dict_type::DictStatus,
    pub is_system: bool,
    pub remark: String,
    pub create_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    pub update_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl From<sys_dict_data::Model> for DictDataVo {
    fn from(model: sys_dict_data::Model) -> Self {
        Self {
            id: model.id,
            dict_type: model.dict_type,
            dict_label: model.dict_label,
            dict_value: model.dict_value,
            dict_sort: model.dict_sort,
            css_class: model.css_class,
            list_class: model.list_class,
            is_default: model.is_default,
            status: model.status,
            is_system: model.is_system,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

/// 简化的字典数据（用于前端下拉框）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictDataSimpleVo {
    pub label: String,
    pub value: String,
    pub list_class: String,
}

impl From<sys_dict_data::Model> for DictDataSimpleVo {
    fn from(model: sys_dict_data::Model) -> Self {
        Self {
            label: model.dict_label,
            value: model.dict_value,
            list_class: model.list_class,
        }
    }
}

/// 全量字典数据（用于前端缓存）
#[derive(Debug, Serialize, JsonSchema)]
pub struct AllDictVo {
    #[serde(flatten)]
    pub data: std::collections::HashMap<String, Vec<DictDataSimpleVo>>,
}
