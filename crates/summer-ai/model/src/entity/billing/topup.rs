//! AI 充值/支付流水表（钱包充值与订阅支付共用）
//! 对应 sql/ai/topup.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 类型：1=在线支付 2=管理员充值 3=兑换码 4=系统赠送 5=订阅购买
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
pub enum TopupType {
    /// 在线支付
    #[sea_orm(num_value = 1)]
    OnlinePayment = 1,
    /// 管理员充值
    #[sea_orm(num_value = 2)]
    AdminTopup = 2,
    /// 兑换码
    #[sea_orm(num_value = 3)]
    RedemptionCode = 3,
    /// 系统赠送
    #[sea_orm(num_value = 4)]
    SystemGrant = 4,
    /// 订阅购买
    #[sea_orm(num_value = 5)]
    SubscriptionPurchase = 5,
}

/// 状态：1=待支付 2=已完成 3=已取消 4=已退款
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
pub enum TopupStatus {
    /// 待支付
    #[sea_orm(num_value = 1)]
    PendingPayment = 1,
    /// 已完成
    #[sea_orm(num_value = 2)]
    Completed = 2,
    /// 已取消
    #[sea_orm(num_value = 3)]
    Cancelled = 3,
    /// 已退款
    #[sea_orm(num_value = 4)]
    Refunded = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "topup")]
pub struct Model {
    /// 流水ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 交易单号（唯一）
    pub trade_no: String,
    /// 订阅套餐ID（0=普通充值）
    pub subscription_plan_id: i64,
    /// 充值额度或订阅授予额度
    pub amount: i64,
    /// 支付金额
    #[sea_orm(column_type = "Decimal(Some((12, 2)))")]
    pub money: BigDecimal,
    /// 货币代码
    pub currency: String,
    /// 支付方式（alipay/wechat/stripe/admin_grant/redemption 等）
    pub payment_method: String,
    /// 类型：1=在线支付 2=管理员充值 3=兑换码 4=系统赠送 5=订阅购买
    pub topup_type: TopupType,
    /// 状态：1=待支付 2=已完成 3=已取消 4=已退款
    pub status: TopupStatus,
    /// 支付网关原始载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub payment_payload: serde_json::Value,
    /// 备注
    pub remark: String,
    /// 完成时间
    pub complete_time: Option<DateTimeWithTimeZone>,
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
