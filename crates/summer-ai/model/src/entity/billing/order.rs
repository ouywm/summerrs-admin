//! AI 订单表
//! 对应 sql/ai/order.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=待支付 2=已支付 3=失败 4=关闭 5=退款
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
pub enum OrderStatus {
    /// 待支付
    #[sea_orm(num_value = 1)]
    PendingPayment = 1,
    /// 已支付
    #[sea_orm(num_value = 2)]
    Paid = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
    /// 关闭
    #[sea_orm(num_value = 4)]
    Closed = 4,
    /// 退款
    #[sea_orm(num_value = 5)]
    Refund = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "order")]
pub struct Model {
    /// 订单ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 关联订阅ID
    pub subscription_id: i64,
    /// 支付方式ID
    pub payment_method_id: i64,
    /// 平台订单号
    pub order_no: String,
    /// 外部交易单号
    pub external_order_no: String,
    /// 订单类型：topup/subscription/refund/manual_adjust/package
    pub order_type: String,
    /// 订单标题
    pub subject: String,
    /// 货币
    pub currency: String,
    /// 订单金额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub amount: BigDecimal,
    /// 对应额度
    pub quota_amount: i64,
    /// 优惠金额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub discount_amount: BigDecimal,
    /// 手续费
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub fee_amount: BigDecimal,
    /// 状态：1=待支付 2=已支付 3=失败 4=关闭 5=退款
    pub status: OrderStatus,
    /// 支付状态
    pub payment_status: String,
    /// 订单来源
    pub source: String,
    /// 扩展信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 支付时间
    pub paid_time: Option<DateTimeWithTimeZone>,
    /// 订单过期时间
    pub expire_time: Option<DateTimeWithTimeZone>,
    /// 关闭时间
    pub close_time: Option<DateTimeWithTimeZone>,
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
