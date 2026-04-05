//! AI 控制面审计日志表实体
//! 记录后台/控制面的配置和权限变更

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 审计结果状态（1=成功 2=拒绝 3=失败）
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
pub enum AuditLogStatus {
    /// 成功
    #[sea_orm(num_value = 1)]
    Success = 1,
    /// 拒绝
    #[sea_orm(num_value = 2)]
    Rejected = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "audit_log")]
pub struct Model {
    /// 审计日志ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID（0=系统级）
    pub organization_id: i64,
    /// 团队ID（0=无）
    pub team_id: i64,
    /// 项目ID（0=无）
    pub project_id: i64,
    /// 操作者类型：user/service_account/system
    pub actor_type: String,
    /// 操作者用户ID
    pub actor_user_id: i64,
    /// 操作者服务账号ID
    pub service_account_id: i64,
    /// 动作编码，如 token.create/channel.update/member.remove
    pub action: String,
    /// 资源类型
    pub resource_type: String,
    /// 资源ID
    pub resource_id: String,
    /// 资源名称冗余
    pub resource_name: String,
    /// 关联请求ID
    pub request_id: String,
    /// 链路追踪ID
    pub trace_id: String,
    /// 字段变更摘要（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub change_set: serde_json::Value,
    /// 客户端IP
    pub ip_address: String,
    /// 客户端UA
    pub user_agent: String,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 结果状态
    pub status: AuditLogStatus,
    /// 记录时间
    pub create_time: DateTimeWithTimeZone,
}
