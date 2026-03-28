use std::collections::HashMap;

use anyhow::Context;
use rand::RngExt;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, Set,
};
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
        endpoint_scope: &str,
        exclude: &[i64],
    ) -> ApiResult<Option<SelectedChannel>> {
        // 查询匹配的 ability 记录
        let mut query = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(group))
            .filter(ability::Column::Enabled.eq(true))
            .filter(ability::Column::EndpointScope.eq(endpoint_scope))
            .order_by_desc(ability::Column::Priority);

        if !model.is_empty() {
            query = query.filter(
                Condition::any()
                    .add(ability::Column::Model.eq(model))
                    .add(ability::Column::Model.eq("*")),
            );
        }

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

        let now = chrono::Utc::now().fixed_offset();
        let mut channel_candidates = build_channel_candidates(&abilities);
        while let Some(channel_id) = select_candidate_from_priorities(&channel_candidates) {
            if let Some(selected) = self.load_selected_channel(channel_id, now).await? {
                return Ok(Some(selected));
            }
            channel_candidates.retain(|(candidate_id, _, _)| *candidate_id != channel_id);
        }

        Ok(None)
    }

    async fn load_selected_channel(
        &self,
        channel_id: i64,
        now: chrono::DateTime<chrono::FixedOffset>,
    ) -> ApiResult<Option<SelectedChannel>> {
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

        let accounts = channel_account::Entity::find()
            .filter(channel_account::Column::ChannelId.eq(channel_id))
            .filter(channel_account::Column::Status.eq(AccountStatus::Enabled))
            .filter(channel_account::Column::Schedulable.eq(true))
            .filter(channel_account::Column::DeletedAt.is_null())
            .order_by_desc(channel_account::Column::Priority)
            .all(&self.db)
            .await
            .context("查询渠道账号失败")
            .map_err(ApiErrors::Internal)?;

        let accounts_by_id: HashMap<i64, channel_account::Model> = accounts
            .into_iter()
            .map(|account| (account.id, account))
            .collect();
        let mut account_candidates: Vec<(i64, i32, i32)> = accounts_by_id
            .values()
            .filter(|account| account_routable(account, now))
            .map(|account| (account.id, account.weight, account.priority))
            .collect();

        while let Some(account_id) = select_candidate_from_priorities(&account_candidates) {
            let Some(account) = accounts_by_id.get(&account_id) else {
                account_candidates.retain(|(candidate_id, _, _)| *candidate_id != account_id);
                continue;
            };
            let api_key =
                crate::service::channel::ChannelService::extract_api_key(&account.credentials);
            if api_key.is_empty() {
                account_candidates.retain(|(candidate_id, _, _)| *candidate_id != account_id);
                continue;
            }

            return Ok(Some(SelectedChannel {
                channel_id: ch.id,
                channel_name: ch.name,
                channel_type: ch.channel_type as i16,
                base_url: ch.base_url,
                model_mapping: ch.model_mapping,
                api_key,
                account_id: account.id,
                account_name: account.name.clone(),
            }));
        }

        Ok(None)
    }

    pub fn record_success_async(&self, selected: &SelectedChannel, elapsed_ms: i32) {
        let db = self.db.clone();
        let selected = selected.clone();
        tokio::spawn(async move {
            let now = chrono::Utc::now().fixed_offset();

            if let Ok(Some(channel)) = channel::Entity::find_by_id(selected.channel_id)
                .one(&db)
                .await
            {
                let was_auto_disabled = channel.status == ChannelStatus::AutoDisabled;
                let mut active: channel::ActiveModel = channel.into();
                active.response_time = Set(elapsed_ms);
                active.failure_streak = Set(0);
                active.last_used_at = Set(Some(now));
                active.last_error_at = Set(None);
                active.last_error_code = Set(String::new());
                active.last_error_message = Set(String::new());
                active.last_health_status = Set(1);
                if was_auto_disabled {
                    active.status = Set(ChannelStatus::Enabled);
                }
                let _ = active.update(&db).await;
            }

            if let Ok(Some(account)) = channel_account::Entity::find_by_id(selected.account_id)
                .one(&db)
                .await
            {
                let mut active: channel_account::ActiveModel = account.into();
                active.response_time = Set(elapsed_ms);
                active.failure_streak = Set(0);
                active.last_used_at = Set(Some(now));
                active.last_error_at = Set(None);
                active.last_error_code = Set(String::new());
                active.last_error_message = Set(String::new());
                let _ = active.update(&db).await;
            }
        });
    }

    pub fn record_failure_async(
        &self,
        selected: &SelectedChannel,
        error_code: impl Into<String>,
        error_message: impl Into<String>,
    ) {
        let db = self.db.clone();
        let selected = selected.clone();
        let error_code = error_code.into();
        let error_message = error_message.into();
        tokio::spawn(async move {
            let now = chrono::Utc::now().fixed_offset();

            if let Ok(Some(channel)) = channel::Entity::find_by_id(selected.channel_id)
                .one(&db)
                .await
            {
                let auto_ban = channel.auto_ban;
                let next_failure_streak = channel.failure_streak.saturating_add(1);
                let mut active: channel::ActiveModel = channel.into();
                active.failure_streak = Set(next_failure_streak);
                active.last_used_at = Set(Some(now));
                active.last_error_at = Set(Some(now));
                active.last_error_code = Set(error_code.clone());
                active.last_error_message = Set(error_message.clone());
                active.last_health_status = Set(-1);
                if auto_ban && next_failure_streak >= 3 {
                    active.status = Set(ChannelStatus::AutoDisabled);
                }
                let _ = active.update(&db).await;
            }

            if let Ok(Some(account)) = channel_account::Entity::find_by_id(selected.account_id)
                .one(&db)
                .await
            {
                let next_failure_streak = account.failure_streak.saturating_add(1);
                let mut active: channel_account::ActiveModel = account.into();
                active.failure_streak = Set(next_failure_streak);
                active.last_used_at = Set(Some(now));
                active.last_error_at = Set(Some(now));
                active.last_error_code = Set(error_code);
                active.last_error_message = Set(error_message);
                let _ = active.update(&db).await;
            }
        });
    }
}

fn build_channel_candidates(abilities: &[ability::Model]) -> Vec<(i64, i32, i32)> {
    let mut candidates: HashMap<i64, (i32, i32)> = HashMap::new();
    for ability in abilities {
        candidates
            .entry(ability.channel_id)
            .and_modify(|entry| {
                if ability.priority > entry.1
                    || (ability.priority == entry.1 && ability.weight > entry.0)
                {
                    *entry = (ability.weight, ability.priority);
                }
            })
            .or_insert((ability.weight, ability.priority));
    }

    candidates
        .into_iter()
        .map(|(channel_id, (weight, priority))| (channel_id, weight, priority))
        .collect()
}

fn select_candidate_from_priorities<T: Copy>(items: &[(T, i32, i32)]) -> Option<T> {
    let max_priority = items.iter().map(|(_, _, priority)| *priority).max()?;
    let weighted_group: Vec<(T, i32)> = items
        .iter()
        .filter(|(_, _, priority)| *priority == max_priority)
        .map(|(item, weight, _)| (*item, *weight))
        .collect();
    weighted_random_select(&weighted_group)
}

fn account_routable(
    account: &channel_account::Model,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    if account.status != AccountStatus::Enabled
        || !account.schedulable
        || account.deleted_at.is_some()
    {
        return false;
    }
    if account
        .expires_at
        .is_some_and(|expires_at| expires_at <= now)
    {
        return false;
    }
    if account
        .rate_limited_until
        .is_some_and(|rate_limited_until| rate_limited_until > now)
    {
        return false;
    }
    if account
        .overload_until
        .is_some_and(|overload_until| overload_until > now)
    {
        return false;
    }
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_account() -> channel_account::Model {
        channel_account::Model {
            id: 7,
            channel_id: 42,
            name: "acct".into(),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({"api_key": "sk-test"}),
            secret_ref: String::new(),
            status: AccountStatus::Enabled,
            schedulable: true,
            priority: 10,
            weight: 5,
            rate_multiplier: 1.into(),
            concurrency_limit: 0,
            quota_limit: 0.into(),
            quota_used: 0.into(),
            balance: 0.into(),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            rate_limited_until: None,
            overload_until: None,
            expires_at: None,
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "tester".into(),
            create_time: chrono::Utc::now().fixed_offset(),
            update_by: "tester".into(),
            update_time: chrono::Utc::now().fixed_offset(),
        }
    }

    #[test]
    fn select_candidate_from_priorities_prefers_highest_priority_group() {
        let selected = select_candidate_from_priorities(&[(11_i64, 1, 100), (22_i64, 50, 10)]);
        assert_eq!(selected, Some(11));
    }

    #[test]
    fn account_routable_rejects_future_windows_and_expiration() {
        let now = chrono::Utc::now().fixed_offset();
        let mut account = sample_account();
        account.rate_limited_until = Some(now + chrono::Duration::minutes(1));
        assert!(!account_routable(&account, now));

        account.rate_limited_until = None;
        account.overload_until = Some(now + chrono::Duration::minutes(1));
        assert!(!account_routable(&account, now));

        account.overload_until = None;
        account.expires_at = Some(now - chrono::Duration::minutes(1));
        assert!(!account_routable(&account, now));
    }
}
