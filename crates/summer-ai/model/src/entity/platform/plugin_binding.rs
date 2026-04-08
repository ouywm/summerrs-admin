//! AI 插件绑定表
//! 对应 sql/ai/plugin_binding.sql

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "plugin_binding")]
pub struct Model {
    /// 绑定ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 插件ID
    pub plugin_id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 路由规则ID
    pub routing_rule_id: i64,
    /// 绑定点：request/response/router/guardrail/audit/scheduler
    pub binding_point: String,
    /// 执行顺序
    pub exec_order: i32,
    /// 是否启用
    pub enabled: bool,
    /// 实例化配置（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub config: serde_json::Value,
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
