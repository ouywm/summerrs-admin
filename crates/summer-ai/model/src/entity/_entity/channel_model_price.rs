//! AI 渠道模型价格表实体
//! 保存渠道当前生效的真实采购价/成本口径

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 计费模式（1=按 token 2=按请求 3=按图片/音频/视频单位）
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
pub enum BillingMode {
    /// 按 token
    #[sea_orm(num_value = 1)]
    PerToken = 1,
    /// 按请求
    #[sea_orm(num_value = 2)]
    PerRequest = 2,
    /// 按单位（图片/音频/视频）
    #[sea_orm(num_value = 3)]
    PerUnit = 3,
}

/// 价格状态（1=启用 2=停用）
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
pub enum PriceStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Active = 1,
    /// 停用
    #[sea_orm(num_value = 2)]
    Inactive = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel_model_price")]
pub struct Model {
    /// 价格 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 渠道 ID
    pub channel_id: i64,
    /// 模型名
    pub model_name: String,
    /// 计费模式
    pub billing_mode: BillingMode,
    /// 价格货币
    pub currency: String,
    /// 价格配置 JSON（input/output/cache/reasoning 等单价）
    #[sea_orm(column_type = "JsonBinary")]
    pub price_config: serde_json::Value,
    /// 价格快照引用 ID
    pub reference_id: String,
    /// 状态
    pub status: PriceStatus,
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
