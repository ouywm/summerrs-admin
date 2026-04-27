use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "idempotency_record")]
pub struct Model {
    /// 幂等记录ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 幂等作用域（如 task.create / callback.midjourney）
    pub scope: String,
    /// 幂等键哈希
    pub idempotency_key_hash: String,
    /// 请求指纹
    pub request_fingerprint: String,
    /// 关联请求ID
    pub request_id: String,
    /// 状态：processing/completed/failed 等
    pub status: String,
    /// 已缓存的响应状态码
    pub response_status: Option<i32>,
    /// 已缓存的响应体
    #[sea_orm(column_type = "Text", nullable)]
    pub response_body: Option<String>,
    /// 失败原因
    pub error_reason: Option<String>,
    /// 处理锁过期时间
    pub locked_until: Option<DateTimeWithTimeZone>,
    /// 记录过期时间
    pub expires_at: DateTimeWithTimeZone,
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
