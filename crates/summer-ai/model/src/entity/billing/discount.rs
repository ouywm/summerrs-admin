//! AI 折扣规则表
//! 对应 sql/ai/discount.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用 3=过期
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
pub enum DiscountStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 过期
    #[sea_orm(num_value = 3)]
    Expired = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "discount")]
pub struct Model {
    /// 折扣ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 作用域：global/organization/project/model/provider/user
    pub scope_type: String,
    /// 作用域键，如模型名/提供方编码
    pub scope_key: String,
    /// 折扣名称
    pub name: String,
    /// 折扣类型：ratio/fixed
    pub discount_type: String,
    /// 折扣值
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub discount_value: BigDecimal,
    /// 货币
    pub currency: String,
    /// 生效条件（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub condition_json: serde_json::Value,
    /// 优先级
    pub priority: i32,
    /// 状态：1=启用 2=禁用 3=过期
    pub status: DiscountStatus,
    /// 开始时间
    pub start_time: Option<DateTimeWithTimeZone>,
    /// 结束时间
    pub end_time: Option<DateTimeWithTimeZone>,
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
