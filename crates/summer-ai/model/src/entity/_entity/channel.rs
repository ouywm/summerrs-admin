//! AI 渠道实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 渠道状态（1=启用, 2=手动禁用, 3=自动禁用, 4=归档）
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
pub enum ChannelStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 手动禁用
    #[sea_orm(num_value = 2)]
    ManualDisabled = 2,
    /// 自动禁用
    #[sea_orm(num_value = 3)]
    AutoDisabled = 3,
    /// 归档
    #[sea_orm(num_value = 4)]
    Archived = 4,
}

/// 渠道类型（1=OpenAI, 3=Anthropic, 14=Azure, 15=Baidu, 17=Ali, 24=Gemini, 28=Ollama）
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
pub enum ChannelType {
    /// OpenAI
    #[sea_orm(num_value = 1)]
    OpenAi = 1,
    /// Anthropic
    #[sea_orm(num_value = 3)]
    Anthropic = 3,
    /// Azure
    #[sea_orm(num_value = 14)]
    Azure = 14,
    /// 百度
    #[sea_orm(num_value = 15)]
    Baidu = 15,
    /// 阿里
    #[sea_orm(num_value = 17)]
    Ali = 17,
    /// Gemini
    #[sea_orm(num_value = 24)]
    Gemini = 24,
    /// Ollama
    #[sea_orm(num_value = 28)]
    Ollama = 28,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 渠道名称
    pub name: String,
    /// 渠道类型
    pub channel_type: ChannelType,
    /// 供应商编码
    pub vendor_code: String,
    /// 基础 URL
    pub base_url: String,
    /// 渠道状态
    pub status: ChannelStatus,
    /// 支持的模型列表
    #[sea_orm(column_type = "JsonBinary")]
    pub models: serde_json::Value,
    /// 模型映射
    #[sea_orm(column_type = "JsonBinary")]
    pub model_mapping: serde_json::Value,
    /// 渠道分组
    pub channel_group: String,
    /// 端点作用域
    #[sea_orm(column_type = "JsonBinary")]
    pub endpoint_scopes: serde_json::Value,
    /// 能力标签
    #[sea_orm(column_type = "JsonBinary")]
    pub capabilities: serde_json::Value,
    /// 权重
    pub weight: i32,
    /// 优先级
    pub priority: i32,
    /// 渠道配置
    #[sea_orm(column_type = "JsonBinary")]
    pub config: serde_json::Value,
    /// 是否自动封禁
    pub auto_ban: bool,
    /// 测试模型
    pub test_model: String,
    /// 已用额度
    pub used_quota: i64,
    /// 余额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub balance: BigDecimal,
    /// 余额更新时间
    pub balance_updated_at: Option<DateTimeWithTimeZone>,
    /// 响应时间（毫秒）
    pub response_time: i32,
    /// 成功率
    #[sea_orm(column_type = "Decimal(Some((8, 4)))")]
    pub success_rate: BigDecimal,
    /// 连续失败次数
    pub failure_streak: i32,
    /// 最后使用时间
    pub last_used_at: Option<DateTimeWithTimeZone>,
    /// 最后错误时间
    pub last_error_at: Option<DateTimeWithTimeZone>,
    /// 最后错误码
    pub last_error_code: String,
    /// 最后错误信息
    #[sea_orm(column_type = "Text")]
    pub last_error_message: String,
    /// 最后健康状态
    pub last_health_status: i16,
    /// 删除时间（软删除）
    pub deleted_at: Option<DateTimeWithTimeZone>,
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
