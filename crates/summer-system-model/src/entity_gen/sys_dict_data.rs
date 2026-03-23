//! 字典数据实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "dict_data")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 字典类型编码
    pub dict_type: String,
    /// 字典标签（显示值）
    pub dict_label: String,
    /// 字典键值（实际值）
    pub dict_value: String,
    /// 排序
    pub dict_sort: i32,
    /// CSS 类名
    pub css_class: String,
    /// 列表样式
    pub list_class: String,
    /// 是否默认选项
    pub is_default: bool,
    /// 状态
    pub status: super::sys_dict_type::DictStatus,
    /// 是否系统内置
    pub is_system: bool,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTime,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTime,
}
