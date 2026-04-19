//! AI 路由目标表
//! 对应 sql/ai/routing_target.sql

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
pub enum RoutingTargetStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "routing_target")]
pub struct Model {
    /// 目标ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属路由规则ID
    pub routing_rule_id: i64,
    /// 目标类型：channel/account/channel_group/plugin/pipeline
    pub target_type: String,
    /// 渠道ID
    pub channel_id: i64,
    /// 账号ID
    pub account_id: i64,
    /// 插件ID
    pub plugin_id: i64,
    /// 目标键
    pub target_key: String,
    /// 权重
    pub weight: i32,
    /// 优先级
    pub priority: i32,
    /// 冷却秒数
    pub cooldown_seconds: i32,
    /// 附加配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub config: serde_json::Value,
    /// 状态：1=启用 2=禁用
    pub status: RoutingTargetStatus,
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
