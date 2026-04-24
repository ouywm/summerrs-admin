use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用
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
pub enum GovernanceBudgetStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "governance_budget")]
pub struct Model {
    /// 预算ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 作用域：organization/team/project/user/token/service_account
    pub scope_type: String,
    /// 作用域ID
    pub scope_id: i64,
    /// 预算名称
    pub budget_name: String,
    /// 货币
    pub currency: String,
    /// 周期：daily/weekly/monthly/custom
    pub period_type: String,
    /// 预算上限
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub limit_amount: BigDecimal,
    /// 预警阈值比例
    #[sea_orm(column_type = "Decimal(Some((10, 4)))")]
    pub warn_threshold: BigDecimal,
    /// 是否硬限制
    pub hard_limit: bool,
    /// 当前已花费
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub spent_amount: BigDecimal,
    /// 预留金额
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub reserved_amount: BigDecimal,
    /// 状态：1=启用 2=禁用
    pub status: GovernanceBudgetStatus,
    /// 上次重置时间
    pub last_reset_time: Option<DateTimeWithTimeZone>,
    /// 下次重置时间
    pub next_reset_time: Option<DateTimeWithTimeZone>,
    /// 扩展信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
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
