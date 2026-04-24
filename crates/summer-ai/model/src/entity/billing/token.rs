use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=已过期 4=额度耗尽
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
    /// 额度耗尽
    #[sea_orm(num_value = 4)]
    QuotaExhausted = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "token")]
pub struct Model {
    /// 令牌ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID（关联 sys."user".id；个人令牌时为拥有者，服务账号令牌时通常为创建者）
    pub user_id: i64,
    /// 绑定的服务账号ID（0 表示用户个人令牌）
    pub service_account_id: i64,
    /// 所属项目ID（0 表示不绑定项目/仅绑定组织或服务账号）
    pub project_id: i64,
    /// 令牌名称（便于识别用途）
    pub name: String,
    /// API Key 的 SHA-256 哈希值
    pub key_hash: String,
    /// API Key 前缀（如 sk-aBcD，用于 UI 展示）
    pub key_prefix: String,
    /// 状态：1=启用 2=禁用 3=已过期 4=额度耗尽
    pub status: TokenStatus,
    /// 剩余配额
    pub remain_quota: i64,
    /// 累计已用配额
    pub used_quota: i64,
    /// 是否不限额度
    pub unlimited_quota: bool,
    /// 允许使用的模型白名单（JSON 数组，空数组=不限制）
    #[sea_orm(column_type = "JsonBinary")]
    pub models: serde_json::Value,
    /// 允许使用的 endpoint 白名单（JSON 数组，空数组=不限制）
    #[sea_orm(column_type = "JsonBinary")]
    pub endpoint_scopes: serde_json::Value,
    /// IP 白名单（JSON 数组，支持 IP/CIDR）
    #[sea_orm(column_type = "JsonBinary")]
    pub ip_whitelist: serde_json::Value,
    /// IP 黑名单（JSON 数组，支持 IP/CIDR）
    #[sea_orm(column_type = "JsonBinary")]
    pub ip_blacklist: serde_json::Value,
    /// 令牌级分组覆盖（为空则跟随 ai.user_quota.channel_group）
    pub group_code_override: String,
    /// 每分钟请求数限制（0=不限制）
    pub rpm_limit: i32,
    /// 每分钟 token 数限制（0=不限制）
    pub tpm_limit: i64,
    /// 并发限制（0=不限制）
    pub concurrency_limit: i32,
    /// 日额度上限（0=不限制）
    pub daily_quota_limit: i64,
    /// 月额度上限（0=不限制）
    pub monthly_quota_limit: i64,
    /// 当前日窗口已用额度
    pub daily_used_quota: i64,
    /// 当前月窗口已用额度
    pub monthly_used_quota: i64,
    /// 当前日窗口起始时间
    pub daily_window_start: Option<DateTimeWithTimeZone>,
    /// 当前月窗口起始时间
    pub monthly_window_start: Option<DateTimeWithTimeZone>,
    /// 过期时间（NULL=永不过期）
    pub expire_time: Option<DateTimeWithTimeZone>,
    /// 最后访问时间
    pub access_time: Option<DateTimeWithTimeZone>,
    /// 最近访问 IP
    pub last_used_ip: String,
    /// 最近访问 UA
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

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Utc::now().fixed_offset();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
