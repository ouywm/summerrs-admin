//! AI 会话表
//! 对应 sql/ai/session.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=活跃 2=过期 3=关闭
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
pub enum SessionStatus {
    /// 活跃
    #[sea_orm(num_value = 1)]
    Active = 1,
    /// 过期
    #[sea_orm(num_value = 2)]
    Expired = 2,
    /// 关闭
    #[sea_orm(num_value = 3)]
    Closed = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "session")]
pub struct Model {
    /// 会话ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 会话键
    pub session_key: String,
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
    /// 客户端类型：web/app/sdk/agent
    pub client_type: String,
    /// 客户端IP
    pub client_ip: String,
    /// 客户端UA
    pub user_agent: String,
    /// 状态：1=活跃 2=过期 3=关闭
    pub status: SessionStatus,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 最后活跃时间
    pub last_active_at: Option<DateTimeWithTimeZone>,
    /// 过期时间
    pub expire_time: Option<DateTimeWithTimeZone>,
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
