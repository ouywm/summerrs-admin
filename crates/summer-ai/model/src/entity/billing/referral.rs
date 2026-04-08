//! AI 邀请返利表
//! 对应 sql/ai/referral.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=待结算 2=已结算 3=失效
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
pub enum ReferralStatus {
    /// 待结算
    #[sea_orm(num_value = 1)]
    PendingSettlement = 1,
    /// 已结算
    #[sea_orm(num_value = 2)]
    Settled = 2,
    /// 失效
    #[sea_orm(num_value = 3)]
    Invalid = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "referral")]
pub struct Model {
    /// 返利记录ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 邀请人用户ID
    pub referrer_user_id: i64,
    /// 邀请人组织ID
    pub referrer_org_id: i64,
    /// 被邀请用户ID
    pub referred_user_id: i64,
    /// 被邀请组织ID
    pub referred_org_id: i64,
    /// 邀请码
    pub invite_code: String,
    /// 状态：1=待结算 2=已结算 3=失效
    pub status: ReferralStatus,
    /// 奖励类型：quota/cash/credit
    pub reward_type: String,
    /// 奖励金额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub reward_amount: BigDecimal,
    /// 奖励额度
    pub reward_quota: i64,
    /// 奖励货币
    pub reward_currency: String,
    /// 结算时间
    pub settled_time: Option<DateTimeWithTimeZone>,
    /// 扩展信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
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
