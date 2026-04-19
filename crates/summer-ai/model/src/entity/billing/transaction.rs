//! AI 账务流水表
//! 对应 sql/ai/transaction.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=成功 2=处理中 3=失败
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
pub enum TransactionStatus {
    /// 成功
    #[sea_orm(num_value = 1)]
    Succeeded = 1,
    /// 处理中
    #[sea_orm(num_value = 2)]
    Processing = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "transaction")]
pub struct Model {
    /// 流水ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 关联订单ID
    pub order_id: i64,
    /// 关联支付方式ID
    pub payment_method_id: i64,
    /// 账本类型：wallet/quota/subscription/referral
    pub account_type: String,
    /// 方向：credit/debit
    pub direction: String,
    /// 交易类型：topup/payment/refund/consume/reward/adjust
    pub trade_type: String,
    /// 金额变动
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub amount: BigDecimal,
    /// 货币
    pub currency: String,
    /// 额度变动
    pub quota_delta: i64,
    /// 变动前余额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub balance_before: BigDecimal,
    /// 变动后余额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub balance_after: BigDecimal,
    /// 参考号
    pub reference_no: String,
    /// 状态：1=成功 2=处理中 3=失败
    pub status: TransactionStatus,
    /// 扩展信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            let now = chrono::Utc::now().fixed_offset();
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
