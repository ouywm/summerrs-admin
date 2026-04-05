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

/// 渠道类型
///
/// 编号设计参考 one-api 惯例，便于数据迁移。
/// `Unknown` 用于向前兼容：新增渠道类型值时只需添加变体，无需迁移。
/// 所有标注 "OpenAI 兼容" 的厂商共享 OpenAI adapter，仅 base_url 不同。
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
    /// 未知
    #[sea_orm(num_value = 0)]
    Unknown = 0,
    /// OpenAI
    #[sea_orm(num_value = 1)]
    OpenAi = 1,
    /// Anthropic
    #[sea_orm(num_value = 3)]
    Anthropic = 3,
    /// Azure OpenAI
    #[sea_orm(num_value = 14)]
    Azure = 14,
    /// 百度（文心一言）
    #[sea_orm(num_value = 15)]
    Baidu = 15,
    /// 阿里（通义千问）— OpenAI 兼容
    #[sea_orm(num_value = 17)]
    Ali = 17,
    /// Google Gemini
    #[sea_orm(num_value = 24)]
    Gemini = 24,
    /// Ollama — OpenAI 兼容（本地部署）
    #[sea_orm(num_value = 28)]
    Ollama = 28,
    /// DeepSeek — OpenAI 兼容
    #[sea_orm(num_value = 30)]
    DeepSeek = 30,
    /// Groq — OpenAI 兼容（极速推理）
    #[sea_orm(num_value = 31)]
    Groq = 31,
    /// Mistral — OpenAI 兼容
    #[sea_orm(num_value = 32)]
    Mistral = 32,
    /// SiliconFlow（硅基流动）— OpenAI 兼容
    #[sea_orm(num_value = 33)]
    SiliconFlow = 33,
    /// vLLM — OpenAI 兼容（自托管）
    #[sea_orm(num_value = 34)]
    Vllm = 34,
    /// Fireworks AI — OpenAI 兼容
    #[sea_orm(num_value = 35)]
    Fireworks = 35,
    /// Together AI — OpenAI 兼容
    #[sea_orm(num_value = 36)]
    Together = 36,
    /// OpenRouter — OpenAI 兼容（聚合路由）
    #[sea_orm(num_value = 37)]
    OpenRouter = 37,
    /// Moonshot（月之暗面）— OpenAI 兼容
    #[sea_orm(num_value = 38)]
    Moonshot = 38,
    /// 零一万物（Yi）— OpenAI 兼容
    #[sea_orm(num_value = 39)]
    Lingyi = 39,
    /// Cohere — 独特协议（未来可选适配）
    #[sea_orm(num_value = 40)]
    Cohere = 40,
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
    #[sea_orm(column_type = "Text", nullable)]
    pub last_error_message: Option<String>,
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
    /// channel -> channel_account（一对多）
    #[sea_orm(has_many)]
    pub channel_accounts: HasMany<super::channel_account::Entity>,
    /// channel -> ability（一对多）
    #[sea_orm(has_many)]
    pub abilities: HasMany<super::ability::Entity>,
}
