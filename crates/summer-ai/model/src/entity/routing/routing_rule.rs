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
pub enum RoutingRuleStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "routing_rule")]
pub struct Model {
    /// 规则ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 规则编码
    pub rule_code: String,
    /// 规则名称
    pub rule_name: String,
    /// 优先级
    pub priority: i32,
    /// 匹配类型：json/header/model/endpoint/expr
    pub match_type: String,
    /// 匹配条件（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub match_conditions: serde_json::Value,
    /// 路由策略：weighted/priority/hash/latency
    pub route_strategy: String,
    /// 失败回退策略
    pub fallback_strategy: String,
    /// 状态：1=启用 2=禁用
    pub status: RoutingRuleStatus,
    /// 开始生效时间
    pub start_time: Option<DateTimeWithTimeZone>,
    /// 结束生效时间
    pub end_time: Option<DateTimeWithTimeZone>,
    /// 扩展元数据（JSON）
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
