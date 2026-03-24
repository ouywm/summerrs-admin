//! AI 令牌实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 令牌状态（1=启用, 2=禁用, 3=已过期, 4=已耗尽）
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
pub enum TokenStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 已过期
    #[sea_orm(num_value = 3)]
    Expired = 3,
    /// 已耗尽
    #[sea_orm(num_value = 4)]
    Exhausted = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "token")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户 ID
    pub user_id: i64,
    /// 服务账号 ID
    pub service_account_id: i64,
    /// 项目 ID
    pub project_id: i64,
    /// 令牌名称
    pub name: String,
    /// 密钥哈希
    #[sea_orm(unique)]
    pub key_hash: String,
    /// 密钥前缀
    pub key_prefix: String,
    /// 令牌状态
    pub status: TokenStatus,
    /// 剩余额度
    pub remain_quota: i64,
    /// 已用额度
    pub used_quota: i64,
    /// 是否无限额度
    pub unlimited_quota: bool,
    /// 允许的模型列表
    #[sea_orm(column_type = "JsonBinary")]
    pub models: serde_json::Value,
    /// 端点作用域
    #[sea_orm(column_type = "JsonBinary")]
    pub endpoint_scopes: serde_json::Value,
    /// IP 白名单
    #[sea_orm(column_type = "JsonBinary")]
    pub ip_whitelist: serde_json::Value,
    /// IP 黑名单
    #[sea_orm(column_type = "JsonBinary")]
    pub ip_blacklist: serde_json::Value,
    /// 分组编码覆盖
    pub group_code_override: String,
    /// 每分钟请求限制
    pub rpm_limit: i32,
    /// 每分钟 Token 限制
    pub tpm_limit: i64,
    /// 并发限制
    pub concurrency_limit: i32,
    /// 每日额度限制
    pub daily_quota_limit: i64,
    /// 每月额度限制
    pub monthly_quota_limit: i64,
    /// 每日已用额度
    pub daily_used_quota: i64,
    /// 每月已用额度
    pub monthly_used_quota: i64,
    /// 每日窗口开始时间
    pub daily_window_start: Option<DateTimeWithTimeZone>,
    /// 每月窗口开始时间
    pub monthly_window_start: Option<DateTimeWithTimeZone>,
    /// 过期时间
    pub expire_time: Option<DateTimeWithTimeZone>,
    /// 最后访问时间
    pub access_time: Option<DateTimeWithTimeZone>,
    /// 最后使用 IP
    pub last_used_ip: String,
    /// 最后 User-Agent
    pub last_user_agent: String,
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
