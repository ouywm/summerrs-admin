use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=未使用 2=已禁用 3=已使用/已发完
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
pub enum RedemptionStatus {
    /// 未使用
    #[sea_orm(num_value = 1)]
    Unused = 1,
    /// 已禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 已使用/已发完
    #[sea_orm(num_value = 3)]
    UsedOrExhausted = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "redemption")]
pub struct Model {
    /// 兑换码ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 兑换码名称/批次备注
    pub name: String,
    /// 兑换码
    pub code: String,
    /// 兑换额度
    pub quota: i64,
    /// 兑换后切换/授予的目标分组（空=不变）
    pub allow_group_code: String,
    /// 状态：1=未使用 2=已禁用 3=已使用/已发完
    pub status: RedemptionStatus,
    /// 可兑换次数
    pub count: i32,
    /// 已兑换次数
    pub used_count: i32,
    /// 最后兑换者用户ID
    pub redeemed_user_id: Option<i64>,
    /// 过期时间
    pub expire_time: Option<DateTimeWithTimeZone>,
    /// 最后一次兑换时间
    pub redeem_time: Option<DateTimeWithTimeZone>,
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
