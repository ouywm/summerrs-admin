use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "usage_billing_dedup")]
pub struct Model {
    /// 去重记录ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 请求唯一标识
    pub request_id: String,
    /// 令牌ID
    pub token_id: i64,
    /// 请求指纹
    pub request_fingerprint: String,
    /// 已结算额度
    pub quota: i64,
    /// 创建时间
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
