use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "guardrail_violation")]
pub struct Model {
    /// 命中记录ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 令牌ID
    pub token_id: i64,
    /// 服务账号ID
    pub service_account_id: i64,
    /// 命中的规则ID
    pub rule_id: i64,
    /// 关联请求ID
    pub request_id: String,
    /// 关联执行尝试ID
    pub execution_id: i64,
    /// 关联消费日志ID
    pub log_id: i64,
    /// 关联异步任务ID
    pub task_id: i64,
    /// 命中阶段
    pub phase: String,
    /// 命中分类
    pub category: String,
    /// 执行动作
    pub action_taken: String,
    /// 关联模型名
    pub model_name: String,
    /// 关联 endpoint
    pub endpoint: String,
    /// 命中的规则模式
    pub matched_pattern: String,
    /// 命中内容哈希
    pub matched_content_hash: String,
    /// 脱敏后的命中片段
    #[sea_orm(column_type = "Text")]
    pub sample_excerpt: String,
    /// 严重级别
    pub severity: i16,
    /// 规则执行耗时
    pub latency_ms: i32,
    /// 扩展信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 记录时间
    pub create_time: DateTimeWithTimeZone,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            let now = chrono::Utc::now().fixed_offset();
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
