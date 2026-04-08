//! AI 消费日志表（单次调用的账务/审计摘要）
//! 对应 sql/ai/log.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 日志类型：1=充值 2=消费 3=管理操作 4=系统
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
pub enum LogType {
    /// 充值
    #[sea_orm(num_value = 1)]
    Topup = 1,
    /// 消费
    #[sea_orm(num_value = 2)]
    Consumption = 2,
    /// 管理操作
    #[sea_orm(num_value = 3)]
    AdminOperation = 3,
    /// 系统
    #[sea_orm(num_value = 4)]
    System = 4,
}

/// 调用状态：1=成功 2=失败 3=取消
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
pub enum LogStatus {
    /// 成功
    #[sea_orm(num_value = 1)]
    Succeeded = 1,
    /// 失败
    #[sea_orm(num_value = 2)]
    Failed = 2,
    /// 取消
    #[sea_orm(num_value = 3)]
    Cancelled = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "log")]
pub struct Model {
    /// 日志ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 调用用户ID
    pub user_id: i64,
    /// 使用的令牌ID
    pub token_id: i64,
    /// 令牌名称冗余
    pub token_name: String,
    /// 所属项目ID（0 表示个人请求）
    pub project_id: i64,
    /// 所属对话ID
    pub conversation_id: i64,
    /// 所属消息ID
    pub message_id: i64,
    /// 所属会话ID
    pub session_id: i64,
    /// 所属线程ID
    pub thread_id: i64,
    /// 所属追踪ID
    pub trace_id: i64,
    /// 实际命中的渠道ID
    pub channel_id: i64,
    /// 渠道名称冗余
    pub channel_name: String,
    /// 实际命中的账号ID（ai.channel_account.id）
    pub account_id: i64,
    /// 账号名称冗余
    pub account_name: String,
    /// 执行尝试ID（对应 ai.request_execution.id，未落库时为0）
    pub execution_id: i64,
    /// 请求 endpoint
    pub endpoint: String,
    /// 协议格式（如 openai/chat_completions）
    pub request_format: String,
    /// 客户端请求模型名
    pub requested_model: String,
    /// 实际转发给上游的模型名
    pub upstream_model: String,
    /// 标准化计费模型名
    pub model_name: String,
    /// 输入 Token 数
    pub prompt_tokens: i32,
    /// 输出 Token 数
    pub completion_tokens: i32,
    /// 总 Token 数
    pub total_tokens: i32,
    /// 缓存命中 Token 数
    pub cached_tokens: i32,
    /// 推理 Token 数
    pub reasoning_tokens: i32,
    /// 本次消耗配额
    pub quota: i64,
    /// 按渠道采购价或成本口径计算的金额
    #[sea_orm(column_type = "Decimal(Some((20, 10)))")]
    pub cost_total: BigDecimal,
    /// 命中的 ai.channel_model_price_version.reference_id
    pub price_reference: String,
    /// 总耗时（毫秒）
    pub elapsed_time: i32,
    /// 首 token 延迟（毫秒）
    pub first_token_time: i32,
    /// 是否流式
    pub is_stream: bool,
    /// 请求唯一标识
    pub request_id: String,
    /// 上游返回的请求ID
    pub upstream_request_id: String,
    /// 最终状态码
    pub status_code: i32,
    /// 客户端 IP
    pub client_ip: String,
    /// 客户端 UA
    pub user_agent: String,
    /// 备注/错误摘要
    #[sea_orm(column_type = "Text")]
    pub content: String,
    /// 日志类型：1=充值 2=消费 3=管理操作 4=系统
    pub log_type: LogType,
    /// 调用状态：1=成功 2=失败 3=取消
    pub status: LogStatus,
    /// 记录时间
    pub create_time: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
