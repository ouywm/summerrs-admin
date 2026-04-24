use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=可用 2=停用 3=失效
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
pub enum PaymentMethodStatus {
    /// 可用
    #[sea_orm(num_value = 1)]
    Available = 1,
    /// 停用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 失效
    #[sea_orm(num_value = 3)]
    Invalid = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "payment_method")]
pub struct Model {
    /// 支付方式ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 支付提供方编码
    pub provider_code: String,
    /// 支付方式类型：card/alipay/wechat/bank_transfer/crypto
    pub method_type: String,
    /// 支付方式显示名称
    pub method_label: String,
    /// 支付平台客户ID
    pub provider_customer_id: String,
    /// 支付平台方式ID
    pub provider_method_id: String,
    /// 状态：1=可用 2=停用 3=失效
    pub status: PaymentMethodStatus,
    /// 是否默认支付方式
    pub is_default: bool,
    /// 过期月份
    pub expire_month: i32,
    /// 过期年份
    pub expire_year: i32,
    /// 账单信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub billing_info: serde_json::Value,
    /// 扩展信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 最后使用时间
    pub last_used_at: Option<DateTimeWithTimeZone>,
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
