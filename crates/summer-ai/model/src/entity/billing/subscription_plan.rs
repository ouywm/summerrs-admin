//! AI 订阅套餐表（长期套餐定义）
//! 对应 sql/ai/subscription_plan.sql

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "subscription_plan")]
pub struct Model {
    /// 套餐ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 套餐编码（唯一）
    pub plan_code: String,
    /// 套餐名称
    pub plan_name: String,
    /// 套餐描述
    pub description: String,
    /// 货币
    pub currency: String,
    /// 售价
    #[sea_orm(column_type = "Decimal(Some((12, 2)))")]
    pub price_amount: BigDecimal,
    /// 套餐总额度（0=无限）
    pub quota_total: i64,
    /// 额度重置周期：never/daily/weekly/monthly/custom
    pub quota_reset_period: String,
    /// 自定义重置天数（非 custom 时为0）
    pub quota_reset_days: i32,
    /// 订阅时长单位：day/month/year/custom
    pub duration_unit: String,
    /// 时长数值
    pub duration_value: i32,
    /// 购买后附加/提升到的分组
    pub group_code: String,
    /// 是否启用
    pub enabled: bool,
    /// 排序
    pub sort_order: i32,
    /// 套餐扩展配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub extra: serde_json::Value,
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
