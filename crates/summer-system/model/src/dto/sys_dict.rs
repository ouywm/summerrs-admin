use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::{sys_dict_data, sys_dict_type};

// ============================================================
// 字典类型 DTO
// ============================================================

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateDictTypeDto {
    #[validate(length(min = 1, max = 100, message = "字典名称长度必须在1-100之间"))]
    pub dict_name: String,
    #[validate(length(min = 1, max = 100, message = "字典类型编码长度必须在1-100之间"))]
    pub dict_type: String,
    pub status: Option<sys_dict_type::DictStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateDictTypeDto {
    pub fn into_active_model(self, operator: String) -> sys_dict_type::ActiveModel {
        sys_dict_type::ActiveModel {
            id: NotSet,
            dict_name: Set(self.dict_name),
            dict_type: Set(self.dict_type),
            status: Set(self.status.unwrap_or(sys_dict_type::DictStatus::Enabled)),
            is_system: Set(false),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.clone()),
            create_time: NotSet,
            update_by: Set(operator),
            update_time: NotSet,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDictTypeDto {
    #[validate(length(min = 1, max = 100, message = "字典名称长度必须在1-100之间"))]
    pub dict_name: Option<String>,
    pub status: Option<sys_dict_type::DictStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateDictTypeDto {
    pub fn apply_to(self, active: &mut sys_dict_type::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(dict_name) = self.dict_name {
            active.dict_name = Set(dict_name);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictTypeQueryDto {
    pub dict_name: Option<String>,
    pub dict_type: Option<String>,
    pub status: Option<sys_dict_type::DictStatus>,
}

impl From<DictTypeQueryDto> for Condition {
    fn from(query: DictTypeQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(name) = query.dict_name {
            cond = cond.add(sys_dict_type::Column::DictName.contains(name));
        }
        if let Some(dict_type) = query.dict_type {
            cond = cond.add(sys_dict_type::Column::DictType.contains(dict_type));
        }
        if let Some(status) = query.status {
            cond = cond.add(sys_dict_type::Column::Status.eq(status));
        }
        cond
    }
}

// ============================================================
// 字典数据 DTO
// ============================================================

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateDictDataDto {
    #[validate(length(min = 1, max = 100, message = "字典类型编码长度必须在1-100之间"))]
    pub dict_type: String,
    #[validate(length(min = 1, max = 100, message = "字典标签长度必须在1-100之间"))]
    pub dict_label: String,
    #[validate(length(min = 1, max = 100, message = "字典键值长度必须在1-100之间"))]
    pub dict_value: String,
    pub dict_sort: Option<i32>,
    #[validate(length(max = 100, message = "CSS类名长度不能超过100"))]
    pub css_class: Option<String>,
    #[validate(length(max = 100, message = "列表样式长度不能超过100"))]
    pub list_class: Option<String>,
    pub is_default: Option<bool>,
    pub status: Option<sys_dict_type::DictStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateDictDataDto {
    pub fn into_active_model(self, operator: String) -> sys_dict_data::ActiveModel {
        sys_dict_data::ActiveModel {
            id: NotSet,
            dict_type: Set(self.dict_type),
            dict_label: Set(self.dict_label),
            dict_value: Set(self.dict_value),
            dict_sort: Set(self.dict_sort.unwrap_or(0)),
            css_class: Set(self.css_class.unwrap_or_default()),
            list_class: Set(self.list_class.unwrap_or_default()),
            is_default: Set(self.is_default.unwrap_or(false)),
            status: Set(self.status.unwrap_or(sys_dict_type::DictStatus::Enabled)),
            is_system: Set(false),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.clone()),
            create_time: NotSet,
            update_by: Set(operator),
            update_time: NotSet,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDictDataDto {
    #[validate(length(min = 1, max = 100, message = "字典标签长度必须在1-100之间"))]
    pub dict_label: Option<String>,
    #[validate(length(min = 1, max = 100, message = "字典键值长度必须在1-100之间"))]
    pub dict_value: Option<String>,
    pub dict_sort: Option<i32>,
    #[validate(length(max = 100, message = "CSS类名长度不能超过100"))]
    pub css_class: Option<String>,
    #[validate(length(max = 100, message = "列表样式长度不能超过100"))]
    pub list_class: Option<String>,
    pub is_default: Option<bool>,
    pub status: Option<sys_dict_type::DictStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateDictDataDto {
    pub fn apply_to(self, active: &mut sys_dict_data::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(dict_label) = self.dict_label {
            active.dict_label = Set(dict_label);
        }
        if let Some(dict_value) = self.dict_value {
            active.dict_value = Set(dict_value);
        }
        if let Some(dict_sort) = self.dict_sort {
            active.dict_sort = Set(dict_sort);
        }
        if let Some(css_class) = self.css_class {
            active.css_class = Set(css_class);
        }
        if let Some(list_class) = self.list_class {
            active.list_class = Set(list_class);
        }
        if let Some(is_default) = self.is_default {
            active.is_default = Set(is_default);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictDataQueryDto {
    pub dict_type: Option<String>,
    pub dict_label: Option<String>,
    pub status: Option<sys_dict_type::DictStatus>,
}

impl From<DictDataQueryDto> for Condition {
    fn from(query: DictDataQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(dict_type) = query.dict_type {
            cond = cond.add(sys_dict_data::Column::DictType.eq(dict_type));
        }
        if let Some(label) = query.dict_label {
            cond = cond.add(sys_dict_data::Column::DictLabel.contains(label));
        }
        if let Some(status) = query.status {
            cond = cond.add(sys_dict_data::Column::Status.eq(status));
        }
        cond
    }
}
