use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::enums::ScheduleType;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, JsonSchema)]
#[sea_orm(schema_name = "sys", table_name = "job")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// 任务ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 租户ID（NULL=全局任务）
    pub tenant_id: Option<i64>,
    /// 任务名称（同租户内唯一）
    pub name: String,
    /// 任务分组
    pub group_name: String,
    /// 任务描述
    pub description: String,
    /// handler 名称（registry key）
    pub handler: String,
    /// 调度类型：CRON / FIXED_RATE / ONESHOT
    pub schedule_type: ScheduleType,
    /// cron 表达式（schedule_type=CRON 时必填，6 字段：秒 分 时 日 月 周）
    pub cron_expr: Option<String>,
    /// 固定间隔毫秒（schedule_type=FIXED_RATE 时必填，> 0）
    pub interval_ms: Option<i64>,
    /// 一次性触发时间（schedule_type=ONESHOT 时必填）
    pub fire_time: Option<DateTime>,
    /// handler 参数（任意 JSON，作为 JobContext.params 注入 handler）
    #[sea_orm(column_type = "JsonBinary")]
    pub params_json: Json,
    /// 是否启用（false 时从调度器摘除）
    pub enabled: bool,
    /// 执行超时毫秒（0=不限，超时后 worker 写 TIMEOUT 终态）
    pub timeout_ms: i64,
    /// 最大重试次数（失败后按指数退避：5s → 10s → 20s …封顶 10 分钟）
    pub retry_max: i32,
    /// 乐观锁版本号（每次更新自增 1）
    pub version: i64,
    /// 创建人用户ID
    pub created_by: Option<i64>,
    /// 创建时间
    pub create_time: DateTime,
    /// 更新时间
    pub update_time: DateTime,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Local::now().naive_local();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
