//! AI 配置项表实体
//! 统一承载 provider/model/plugin/system 等配置

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 配置状态（1=启用 2=禁用）
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize_repr,
    Deserialize_repr,
    JsonSchema,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum ConfigEntryStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "config_entry")]
pub struct Model {
    /// 配置项ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 作用域：system/organization/project/provider/model/plugin
    pub scope_type: String,
    /// 作用域ID
    pub scope_id: i64,
    /// 配置分类
    pub category: String,
    /// 配置键
    pub config_key: String,
    /// 配置值（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub config_value: serde_json::Value,
    /// 敏感值外部引用
    pub secret_ref: String,
    /// 状态
    pub status: ConfigEntryStatus,
    /// 版本号
    pub version_no: i32,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}
