//! AI 能力路由实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "ability")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 渠道分组
    pub channel_group: String,
    /// 端点作用域
    pub endpoint_scope: String,
    /// 模型名称
    pub model: String,
    /// 渠道 ID
    pub channel_id: i64,
    /// 是否启用
    pub enabled: bool,
    /// 优先级
    pub priority: i32,
    /// 权重
    pub weight: i32,
    /// 路由配置
    #[sea_orm(column_type = "JsonBinary")]
    pub route_config: serde_json::Value,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
    /// 关联渠道（多对一）
    #[sea_orm(belongs_to, from = "channel_id", to = "id", skip_fk)]
    pub channel: Option<super::channel::Entity>,
}
