//! AI 请求日志实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 日志类型（1=充值, 2=消费, 3=管理, 4=系统）
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
    Recharge = 1,
    /// 消费
    #[sea_orm(num_value = 2)]
    Consume = 2,
    /// 管理
    #[sea_orm(num_value = 3)]
    Management = 3,
    /// 系统
    #[sea_orm(num_value = 4)]
    System = 4,
}

/// 日志状态（1=成功, 2=失败, 3=已取消）
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
    Success = 1,
    /// 失败
    #[sea_orm(num_value = 2)]
    Failed = 2,
    /// 已取消
    #[sea_orm(num_value = 3)]
    Cancelled = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "log")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户 ID
    pub user_id: i64,
    /// 令牌 ID
    pub token_id: i64,
    /// 令牌名称
    pub token_name: String,
    /// 项目 ID
    pub project_id: i64,
    /// 会话 ID
    pub conversation_id: i64,
    /// 消息 ID
    pub message_id: i64,
    /// 会话 ID
    pub session_id: i64,
    /// 线程 ID
    pub thread_id: i64,
    /// 追踪 ID
    pub trace_id: i64,
    /// 渠道 ID
    pub channel_id: i64,
    /// 渠道名称
    pub channel_name: String,
    /// 账号 ID
    pub account_id: i64,
    /// 账号名称
    pub account_name: String,
    /// 执行 ID
    pub execution_id: i64,
    /// 端点
    pub endpoint: String,
    /// 请求格式
    pub request_format: String,
    /// 请求模型
    pub requested_model: String,
    /// 上游模型
    pub upstream_model: String,
    /// 模型名称
    pub model_name: String,
    /// 提示 Token 数
    pub prompt_tokens: i32,
    /// 补全 Token 数
    pub completion_tokens: i32,
    /// 总 Token 数
    pub total_tokens: i32,
    /// 缓存 Token 数
    pub cached_tokens: i32,
    /// 推理 Token 数
    pub reasoning_tokens: i32,
    /// 额度消耗
    pub quota: i64,
    /// 总费用
    #[sea_orm(column_type = "Decimal(Some((20, 10)))")]
    pub cost_total: BigDecimal,
    /// 价格参考
    pub price_reference: String,
    /// 耗时（毫秒）
    pub elapsed_time: i32,
    /// 首 Token 时间（毫秒）
    pub first_token_time: i32,
    /// 是否流式
    pub is_stream: bool,
    /// 请求 ID
    pub request_id: String,
    /// 上游请求 ID
    pub upstream_request_id: String,
    /// HTTP 状态码
    pub status_code: i32,
    /// 客户端 IP
    pub client_ip: String,
    /// User-Agent
    pub user_agent: String,
    /// 请求/响应内容
    #[sea_orm(column_type = "Text")]
    pub content: String,
    /// 日志类型
    pub log_type: LogType,
    /// 日志状态
    pub status: LogStatus,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
}
