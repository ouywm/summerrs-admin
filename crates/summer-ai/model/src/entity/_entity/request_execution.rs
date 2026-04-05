//! AI 请求执行尝试实体
//! 一次 request 可能重试多个渠道/账号，每次尝试记录一条

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 执行状态（1=待执行, 2=执行中, 3=成功, 4=失败, 5=取消）
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
pub enum ExecutionStatus {
    /// 待执行
    #[sea_orm(num_value = 1)]
    Pending = 1,
    /// 执行中
    #[sea_orm(num_value = 2)]
    Running = 2,
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
#[sea_orm(schema_name = "ai", table_name = "request_execution")]
pub struct Model {
    /// 执行尝试 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属请求主键（ai.request.id）
    pub ai_request_id: i64,
    /// 请求唯一标识冗余
    pub request_id: String,
    /// 第几次尝试（从 1 开始）
    pub attempt_no: i32,
    /// 命中的渠道 ID
    pub channel_id: i64,
    /// 命中的账号 ID
    pub account_id: i64,
    /// 此次尝试的端点
    pub endpoint: String,
    /// 此次尝试的上游协议格式
    pub request_format: String,
    /// 客户端请求模型
    pub requested_model: String,
    /// 转发给上游的模型
    pub upstream_model: String,
    /// 上游请求 ID
    pub upstream_request_id: String,
    /// 上游请求头快照（脱敏后）
    #[sea_orm(column_type = "JsonBinary")]
    pub request_headers: serde_json::Value,
    /// 发给上游的真实请求体
    #[sea_orm(column_type = "JsonBinary")]
    pub request_body: serde_json::Value,
    /// 上游返回的响应体
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub response_body: Option<serde_json::Value>,
    /// 上游状态码
    pub response_status_code: i32,
    /// 执行状态
    pub status: ExecutionStatus,
    /// 失败摘要
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    /// 此次尝试耗时（毫秒）
    pub duration_ms: i32,
    /// 此次尝试首 token 延迟（毫秒）
    pub first_token_ms: i32,
    /// 开始时间
    pub started_at: DateTimeWithTimeZone,
    /// 结束时间
    pub finished_at: Option<DateTimeWithTimeZone>,
    /// 记录创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 关联请求主表（多对一，逻辑关联 ai.request.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "ai_request_id", to = "id", skip_fk)]
    pub request: Option<super::request::Entity>,
}
