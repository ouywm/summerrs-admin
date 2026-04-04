//! AI 渠道模型价格版本表实体
//! 每次改价都保留历史快照，便于账务回放与毛利分析

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 版本状态（1=生效 2=归档）
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
pub enum PriceVersionStatus {
    /// 生效
    #[sea_orm(num_value = 1)]
    Active = 1,
    /// 归档
    #[sea_orm(num_value = 2)]
    Archived = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel_model_price_version")]
pub struct Model {
    /// 价格版本 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 主价格 ID
    pub channel_model_price_id: i64,
    /// 渠道 ID 冗余
    pub channel_id: i64,
    /// 模型名冗余
    pub model_name: String,
    /// 版本号
    pub version_no: i32,
    /// 价格快照引用 ID
    pub reference_id: String,
    /// 价格配置 JSON 快照
    #[sea_orm(column_type = "JsonBinary")]
    pub price_config: serde_json::Value,
    /// 生效开始时间
    pub effective_start_at: DateTimeWithTimeZone,
    /// 生效结束时间（NULL=当前仍生效）
    pub effective_end_at: Option<DateTimeWithTimeZone>,
    /// 状态
    pub status: PriceVersionStatus,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
}
