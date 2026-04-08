//! AI RBAC 策略版本表
//! 对应 sql/ai/rbac_policy_version.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=草稿 2=生效 3=归档
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
pub enum RbacPolicyVersionStatus {
    /// 草稿
    #[sea_orm(num_value = 1)]
    Draft = 1,
    /// 生效
    #[sea_orm(num_value = 2)]
    Effective = 2,
    /// 归档
    #[sea_orm(num_value = 3)]
    Archived = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "rbac_policy_version")]
pub struct Model {
    /// 策略版本ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属策略ID
    pub policy_id: i64,
    /// 版本号
    pub version_no: i32,
    /// 变更摘要
    pub change_summary: String,
    /// 策略文档（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub policy_document: serde_json::Value,
    /// 状态：1=草稿 2=生效 3=归档
    pub status: RbacPolicyVersionStatus,
    /// 发布人
    pub published_by: String,
    /// 发布时间
    pub published_at: Option<DateTimeWithTimeZone>,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
