//! AI 订阅预扣记录表
//! 对应 sql/ai/subscription_preconsume_record.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=预扣中 2=已结算 3=已释放 4=过期
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
pub enum SubscriptionPreconsumeRecordStatus {
    /// 预扣中
    #[sea_orm(num_value = 1)]
    Reserved = 1,
    /// 已结算
    #[sea_orm(num_value = 2)]
    Settled = 2,
    /// 已释放
    #[sea_orm(num_value = 3)]
    Released = 3,
    /// 过期
    #[sea_orm(num_value = 4)]
    Expired = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "subscription_preconsume_record")]
pub struct Model {
    /// 预扣记录ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户订阅ID
    pub user_subscription_id: i64,
    /// 关联请求ID
    pub request_id: String,
    /// 关联任务ID
    pub task_id: i64,
    /// 状态：1=预扣中 2=已结算 3=已释放 4=过期
    pub status: SubscriptionPreconsumeRecordStatus,
    /// 预留额度
    pub reserved_quota: i64,
    /// 最终结算额度
    pub settled_quota: i64,
    /// 预留金额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub reserved_amount: BigDecimal,
    /// 结算金额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub settled_amount: BigDecimal,
    /// 预扣失效时间
    pub expire_time: Option<DateTimeWithTimeZone>,
    /// 结算时间
    pub settle_time: Option<DateTimeWithTimeZone>,
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
