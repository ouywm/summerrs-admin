use anyhow::Context;
use rand::RngExt;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_sea_orm::DbConn;

use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_common::error::{ApiErrors, ApiResult};

/// 渠道路由后选中的上游信息
#[derive(Debug, Clone)]
pub struct SelectedChannel {
    pub channel_id: i64,
    pub channel_name: String,
    pub channel_type: i16,
    pub base_url: String,
    pub model_mapping: serde_json::Value,
    pub api_key: String,
    pub account_id: i64,
    pub account_name: String,
}

#[derive(Clone, Service)]
pub struct ChannelRouter {
    #[inject(component)]
    db: DbConn,
}

impl ChannelRouter {
    /// 选择一个可用渠道
    ///
    /// 1. 查 ability 表: channel_group + model + enabled
    /// 2. JOIN channel: status = Enabled
    /// 3. 按 priority DESC 分组，取最高优先级组
    /// 4. 组内按 weight 加权随机
    /// 5. 从 channel_account 取可用 API Key
    pub async fn select_channel(
        &self,
        group: &str,
        model: &str,
        _endpoint_scope: &str,
        exclude: &[i64],
    ) -> ApiResult<Option<SelectedChannel>> {
        // 查询匹配的 ability 记录
        let mut query = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(group))
            .filter(ability::Column::Model.eq(model))
            .filter(ability::Column::Enabled.eq(true))
            .order_by_desc(ability::Column::Priority);

        // 排除渠道
        if !exclude.is_empty() {
            query = query.filter(ability::Column::ChannelId.is_not_in(exclude.to_vec()));
        }

        let abilities = query
            .all(&self.db)
            .await
            .context("查询渠道路由失败")
            .map_err(ApiErrors::Internal)?;

        if abilities.is_empty() {
            return Ok(None);
        }

        // 按 priority 分组，取最高优先级
        let max_priority = abilities[0].priority;
        let top_group: Vec<&ability::Model> = abilities
            .iter()
            .filter(|a| a.priority == max_priority)
            .collect();

        // 加权随机选择
        let items: Vec<(i64, i32)> = top_group.iter().map(|a| (a.channel_id, a.weight)).collect();

        let channel_id = match weighted_random_select(&items) {
            Some(id) => id,
            None => return Ok(None),
        };

        // 查询渠道信息
        let ch = channel::Entity::find_by_id(channel_id)
            .filter(channel::Column::Status.eq(ChannelStatus::Enabled))
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道详情失败")
            .map_err(ApiErrors::Internal)?;

        let ch = match ch {
            Some(c) => c,
            None => return Ok(None),
        };

        // 获取可用账号
        let account = channel_account::Entity::find()
            .filter(channel_account::Column::ChannelId.eq(channel_id))
            .filter(channel_account::Column::Status.eq(AccountStatus::Enabled))
            .filter(channel_account::Column::Schedulable.eq(true))
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道账号失败")
            .map_err(ApiErrors::Internal)?;

        let account = match account {
            Some(a) => a,
            None => return Ok(None),
        };

        let api_key =
            crate::service::channel::ChannelService::extract_api_key(&account.credentials);

        Ok(Some(SelectedChannel {
            channel_id: ch.id,
            channel_name: ch.name,
            channel_type: ch.channel_type as i16,
            base_url: ch.base_url,
            model_mapping: ch.model_mapping,
            api_key,
            account_id: account.id,
            account_name: account.name,
        }))
    }
}

/// 加权随机选择
fn weighted_random_select<T: Copy>(items: &[(T, i32)]) -> Option<T> {
    let total: i32 = items.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return items.first().map(|(t, _)| *t);
    }
    let mut pick = rand::rng().random_range(0..total);
    for (item, weight) in items {
        if pick < *weight {
            return Some(*item);
        }
        pick -= weight;
    }
    None
}
