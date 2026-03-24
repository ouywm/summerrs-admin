//! AI 模型配置实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 模型类型（1=对话, 2=嵌入, 3=图像, 4=音频, 5=推理）
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
pub enum ModelType {
    /// 对话
    #[sea_orm(num_value = 1)]
    Chat = 1,
    /// 嵌入
    #[sea_orm(num_value = 2)]
    Embedding = 2,
    /// 图像
    #[sea_orm(num_value = 3)]
    Image = 3,
    /// 音频
    #[sea_orm(num_value = 4)]
    Audio = 4,
    /// 推理
    #[sea_orm(num_value = 5)]
    Reasoning = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "model_config")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 模型名称（唯一标识）
    #[sea_orm(unique)]
    pub model_name: String,
    /// 显示名称
    pub display_name: String,
    /// 模型类型
    pub model_type: ModelType,
    /// 供应商编码
    pub vendor_code: String,
    /// 支持的端点
    #[sea_orm(column_type = "JsonBinary")]
    pub supported_endpoints: serde_json::Value,
    /// 输入价格倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub input_ratio: BigDecimal,
    /// 输出价格倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub output_ratio: BigDecimal,
    /// 缓存输入价格倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub cached_input_ratio: BigDecimal,
    /// 推理价格倍率
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub reasoning_ratio: BigDecimal,
    /// 能力标签
    #[sea_orm(column_type = "JsonBinary")]
    pub capabilities: serde_json::Value,
    /// 最大上下文长度
    pub max_context: i32,
    /// 货币类型
    pub currency: String,
    /// 生效时间
    pub effective_from: Option<DateTimeWithTimeZone>,
    /// 元数据
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 是否启用
    pub enabled: bool,
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
