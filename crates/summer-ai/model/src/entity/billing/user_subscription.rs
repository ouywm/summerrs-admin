//! AI 用户订阅表（用户实际拥有的订阅实例）
//! 对应 sql/ai/user_subscription.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=生效中 2=已过期 3=已取消 4=额度耗尽
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
pub enum UserSubscriptionStatus {
    /// 生效中
    #[sea_orm(num_value = 1)]
    Active = 1,
    /// 已过期
    #[sea_orm(num_value = 2)]
    Expired = 2,
    /// 已取消
    #[sea_orm(num_value = 3)]
    Cancelled = 3,
    /// 额度耗尽
    #[sea_orm(num_value = 4)]
    QuotaExhausted = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "user_subscription")]
pub struct Model {
    /// 用户订阅ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 套餐ID
    pub plan_id: i64,
    /// 状态：1=生效中 2=已过期 3=已取消 4=额度耗尽
    pub status: UserSubscriptionStatus,
    /// 订阅总额度
    pub quota_total: i64,
    /// 订阅已用额度
    pub quota_used: i64,
    /// 当前日窗口已用额度
    pub daily_used_quota: i64,
    /// 当前月窗口已用额度
    pub monthly_used_quota: i64,
    /// 生效开始时间
    pub start_time: DateTimeWithTimeZone,
    /// 到期时间
    pub expire_time: DateTimeWithTimeZone,
    /// 上次额度重置时间
    pub last_reset_time: Option<DateTimeWithTimeZone>,
    /// 下次额度重置时间
    pub next_reset_time: Option<DateTimeWithTimeZone>,
    /// 订阅生效时的分组快照
    pub group_code_snapshot: String,
    /// 来源交易单号
    pub source_trade_no: String,
    /// 分配人ID（管理员分配时使用）
    pub assigned_by: i64,
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
