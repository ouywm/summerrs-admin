//! AI 请求主表实体
//! 记录面向客户端的一次完整请求

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 请求状态（1=待处理, 2=处理中, 3=成功, 4=失败, 5=取消）
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
pub enum RequestStatus {
    /// 待处理
    #[sea_orm(num_value = 1)]
    Pending = 1,
    /// 处理中
    #[sea_orm(num_value = 2)]
    Processing = 2,
    /// 成功
    #[sea_orm(num_value = 3)]
    Success = 3,
    /// 失败
    #[sea_orm(num_value = 4)]
    Failed = 4,
    /// 取消
    #[sea_orm(num_value = 5)]
    Cancelled = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "request")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 请求唯一标识
    #[sea_orm(unique)]
    pub request_id: String,
    /// 调用用户 ID
    pub user_id: i64,
    /// 调用令牌 ID
    pub token_id: i64,
    /// 所属项目 ID
    pub project_id: i64,
    /// 所属对话 ID
    pub conversation_id: i64,
    /// 触发本次请求的消息 ID
    pub message_id: i64,
    /// 所属会话 ID
    pub session_id: i64,
    /// 所属线程 ID
    pub thread_id: i64,
    /// 所属追踪 ID
    pub trace_id: i64,
    /// 命中的渠道分组
    pub channel_group: String,
    /// 来源类型（api/playground/test/task）
    pub source_type: String,
    /// 请求端点
    pub endpoint: String,
    /// 外部协议格式
    pub request_format: String,
    /// 客户端请求模型
    pub requested_model: String,
    /// 最终映射后的上游模型
    pub upstream_model: String,
    /// 是否流式
    pub is_stream: bool,
    /// 客户端 IP
    pub client_ip: String,
    /// 客户端 User-Agent
    pub user_agent: String,
    /// 请求头快照（脱敏后）
    #[sea_orm(column_type = "JsonBinary")]
    pub request_headers: serde_json::Value,
    /// 请求体快照
    #[sea_orm(column_type = "JsonBinary")]
    pub request_body: serde_json::Value,
    /// 客户端最终收到的响应体
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub response_body: Option<serde_json::Value>,
    /// 返回给客户端的状态码
    pub response_status_code: i32,
    /// 请求状态
    pub status: RequestStatus,
    /// 错误摘要
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    /// 总耗时（毫秒）
    pub duration_ms: i32,
    /// 首 token 延迟（毫秒）
    pub first_token_ms: i32,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}
