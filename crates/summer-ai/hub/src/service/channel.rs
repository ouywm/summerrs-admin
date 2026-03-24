use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_sea_orm::DbConn;

use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_common::error::{ApiErrors, ApiResult};

/// 渠道查询相关的简单封装
#[derive(Clone, Service)]
pub struct ChannelService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelService {
    /// 根据 ID 获取渠道
    pub async fn get_by_id(&self, id: i64) -> ApiResult<Option<channel::Model>> {
        channel::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询渠道失败")
            .map_err(ApiErrors::Internal)
    }

    /// 获取渠道的可用账号（取 API Key）
    pub async fn get_schedulable_account(
        &self,
        channel_id: i64,
    ) -> ApiResult<Option<channel_account::Model>> {
        channel_account::Entity::find()
            .filter(channel_account::Column::ChannelId.eq(channel_id))
            .filter(channel_account::Column::Status.eq(AccountStatus::Enabled))
            .filter(channel_account::Column::Schedulable.eq(true))
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道账号失败")
            .map_err(ApiErrors::Internal)
    }

    /// 从 credentials JSON 中提取 API Key
    pub fn extract_api_key(credentials: &serde_json::Value) -> String {
        credentials
            .get("api_key")
            .or_else(|| credentials.get("apiKey"))
            .or_else(|| credentials.get("key"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }
}
