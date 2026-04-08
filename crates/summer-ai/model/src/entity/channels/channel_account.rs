//! AI 渠道账号/密钥池表（一个渠道下的实际可调度账号）
//! 对应 sql/ai/channel_account.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=额度耗尽 4=过期 5=冷却中
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
pub enum ChannelAccountStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 额度耗尽
    #[sea_orm(num_value = 3)]
    QuotaExhausted = 3,
    /// 过期
    #[sea_orm(num_value = 4)]
    Expired = 4,
    /// 冷却中
    #[sea_orm(num_value = 5)]
    CoolingDown = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel_account")]
pub struct Model {
    /// 账号ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属渠道ID（ai.channel.id）
    pub channel_id: i64,
    /// 账号名称（便于识别具体 Key/OAuth 账号）
    pub name: String,
    /// 凭证类型：api_key/oauth/cookie/session/token 等
    pub credential_type: String,
    /// 凭证载荷（JSON，如 {"api_key": "..."}、OAuth token、cookie 等）
    #[sea_orm(column_type = "JsonBinary")]
    pub credentials: serde_json::Value,
    /// 外部密钥管理引用（如 Vault/KMS 路径），为空表示直接落库 credentials
    pub secret_ref: String,
    /// 状态：1=启用 2=禁用 3=额度耗尽 4=过期 5=冷却中
    pub status: ChannelAccountStatus,
    /// 当前是否允许被路由器调度
    pub schedulable: bool,
    /// 账号优先级（同渠道内可二次调度）
    pub priority: i32,
    /// 账号权重（同优先级内加权随机）
    pub weight: i32,
    /// 账号级成本倍率快照，可用于不同账号不同采购价
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub rate_multiplier: BigDecimal,
    /// 并发上限（0=不限制）
    pub concurrency_limit: i32,
    /// 账号总额度上限（0=未知/不限制）
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub quota_limit: BigDecimal,
    /// 账号已用额度
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub quota_used: BigDecimal,
    /// 账号级余额快照
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub balance: BigDecimal,
    /// 账号余额更新时间
    pub balance_updated_at: Option<DateTimeWithTimeZone>,
    /// 最近测速响应时间（毫秒）
    pub response_time: i32,
    /// 连续失败次数
    pub failure_streak: i32,
    /// 最近一次实际使用时间
    pub last_used_at: Option<DateTimeWithTimeZone>,
    /// 最近错误时间
    pub last_error_at: Option<DateTimeWithTimeZone>,
    /// 最近错误码
    pub last_error_code: String,
    /// 最近错误摘要
    #[sea_orm(column_type = "Text")]
    pub last_error_message: String,
    /// 速率限制冷却到期时间
    pub rate_limited_until: Option<DateTimeWithTimeZone>,
    /// 上游过载冷却到期时间
    pub overload_until: Option<DateTimeWithTimeZone>,
    /// 账号凭证失效时间
    pub expires_at: Option<DateTimeWithTimeZone>,
    /// 账号级测速模型
    pub test_model: String,
    /// 最近测速时间
    pub test_time: Option<DateTimeWithTimeZone>,
    /// 账号级扩展字段（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub extra: serde_json::Value,
    /// 软删除时间
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

    /// 关联渠道（多对一，逻辑关联 ai.channel.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "channel_id", to = "id", skip_fk)]
    /// channel
    pub channel: Option<super::channel::Entity>,
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
