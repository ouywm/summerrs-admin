use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_core::types::model::{ModelListResponse, ModelObject};
use summer_ai_model::dto::model_config::{
    CreateModelConfigDto, QueryModelConfigDto, UpdateModelConfigDto,
};
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_ai_model::entity::model_config;
use summer_ai_model::vo::model_config::ModelConfigVo;

use crate::relay::billing::model_config_cache_key;
use crate::service::channel::ChannelService;
use crate::service::runtime_cache::RuntimeCacheService;

#[derive(Clone, Service)]
pub struct ModelService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    cache: RuntimeCacheService,
    #[inject(component)]
    channel_svc: ChannelService,
}

impl ModelService {
    pub async fn list_available(&self, group: &str) -> ApiResult<ModelListResponse> {
        let abilities = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(group))
            .filter(ability::Column::Enabled.eq(true))
            .all(&self.db)
            .await
            .context("查询可用模型失败")
            .map_err(ApiErrors::Internal)?;

        if abilities.is_empty() {
            return Ok(ModelListResponse {
                object: "list".into(),
                data: Vec::new(),
            });
        }

        let channel_ids: Vec<i64> = abilities.iter().map(|ability| ability.channel_id).collect();
        let enabled_channels = channel::Entity::find()
            .filter(channel::Column::Id.is_in(channel_ids.clone()))
            .filter(channel::Column::Status.eq(ChannelStatus::Enabled))
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("查询启用渠道失败")
            .map_err(ApiErrors::Internal)?;

        let enabled_channel_ids: std::collections::HashSet<i64> = enabled_channels
            .into_iter()
            .map(|channel| channel.id)
            .collect();
        if enabled_channel_ids.is_empty() {
            return Ok(ModelListResponse {
                object: "list".into(),
                data: Vec::new(),
            });
        }

        let now = chrono::Utc::now().fixed_offset();
        let active_accounts = channel_account::Entity::find()
            .filter(
                channel_account::Column::ChannelId
                    .is_in(enabled_channel_ids.iter().copied().collect::<Vec<_>>()),
            )
            .filter(channel_account::Column::Status.eq(AccountStatus::Enabled))
            .filter(channel_account::Column::Schedulable.eq(true))
            .filter(channel_account::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("查询可调度渠道账号失败")
            .map_err(ApiErrors::Internal)?;

        let active_channel_ids: std::collections::HashSet<i64> = active_accounts
            .into_iter()
            .filter(|account| account_is_available_for_model_listing(account, now))
            .map(|account| account.channel_id)
            .collect();

        let model_names = available_model_names(abilities, &active_channel_ids);

        let configs = model_config::Entity::find()
            .filter(model_config::Column::Enabled.eq(true))
            .all(&self.db)
            .await
            .context("查询模型配置失败")
            .map_err(ApiErrors::Internal)?;

        let config_map: std::collections::HashMap<String, &model_config::Model> =
            configs.iter().map(|c| (c.model_name.clone(), c)).collect();

        let data: Vec<ModelObject> = model_names
            .into_iter()
            .map(|name| {
                let cfg = config_map.get(&name);
                ModelObject {
                    id: name.clone(),
                    object: "model".into(),
                    created: cfg.map(|c| c.create_time.timestamp()).unwrap_or(0),
                    owned_by: cfg
                        .map(|c| c.vendor_code.clone())
                        .unwrap_or_else(|| "unknown".into()),
                }
            })
            .collect();

        Ok(ModelListResponse {
            object: "list".into(),
            data,
        })
    }

    pub async fn get_available(
        &self,
        group: &str,
        model_id: &str,
    ) -> ApiResult<Option<ModelObject>> {
        let list = self.list_available(group).await?;
        Ok(list.data.into_iter().find(|model| model.id == model_id))
    }

    pub async fn list_configs(
        &self,
        query: QueryModelConfigDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ModelConfigVo>> {
        let page = model_config::Entity::find()
            .filter(query)
            .order_by_desc(model_config::Column::UpdateTime)
            .order_by_desc(model_config::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询模型配置列表失败")?;

        Ok(page.map(ModelConfigVo::from_model))
    }

    pub async fn get_config(&self, id: i64) -> ApiResult<ModelConfigVo> {
        Ok(ModelConfigVo::from_model(self.find_config_model(id).await?))
    }

    pub async fn create_config(&self, dto: CreateModelConfigDto, operator: &str) -> ApiResult<()> {
        let existing = model_config::Entity::find()
            .filter(model_config::Column::ModelName.eq(&dto.model_name))
            .one(&self.db)
            .await
            .context("检查模型配置是否重复失败")?;

        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "模型配置已存在: {}",
                dto.model_name
            )));
        }

        let cache_key = model_config_cache_key(&dto.model_name);
        let model_name = dto.model_name.clone();
        dto.into_active_model(operator)
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建模型配置失败")?;
        let _ = self.cache.delete(&cache_key).await;
        self.channel_svc
            .resync_abilities_for_model_name(&model_name)
            .await?;
        Ok(())
    }

    pub async fn update_config(
        &self,
        id: i64,
        dto: UpdateModelConfigDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_config_model(id).await?;
        let cache_key = model_config_cache_key(&model.model_name);
        let model_name = model.model_name.clone();
        let mut active: model_config::ActiveModel = model.into();
        dto.apply_to(&mut active, operator)
            .map_err(ApiErrors::BadRequest)?;
        active.update(&self.db).await.context("更新模型配置失败")?;
        let _ = self.cache.delete(&cache_key).await;
        self.channel_svc
            .resync_abilities_for_model_name(&model_name)
            .await?;
        Ok(())
    }

    async fn find_config_model(&self, id: i64) -> ApiResult<model_config::Model> {
        model_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询模型配置详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("模型配置不存在".to_string()))
    }
}

fn available_model_names(
    abilities: Vec<ability::Model>,
    active_channel_ids: &std::collections::HashSet<i64>,
) -> Vec<String> {
    let mut model_names: Vec<String> = abilities
        .into_iter()
        .filter(|ability| active_channel_ids.contains(&ability.channel_id))
        .map(|ability| ability.model)
        .collect();
    model_names.sort();
    model_names.dedup();
    model_names
}

fn account_is_available_for_model_listing(
    account: &channel_account::Model,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    account.expires_at.is_none_or(|expires_at| expires_at > now)
        && account
            .rate_limited_until
            .is_none_or(|recover_at| recover_at <= now)
        && account
            .overload_until
            .is_none_or(|recover_at| recover_at <= now)
        && !ChannelService::extract_api_key(&account.credentials).is_empty()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{account_is_available_for_model_listing, available_model_names};
    use summer_ai_model::entity::{ability, channel_account};

    #[test]
    fn available_model_names_keeps_models_from_non_chat_scopes() {
        let names = available_model_names(
            vec![
                sample_ability(11, "chat", "gpt-5.4"),
                sample_ability(11, "responses", "gpt-5.4"),
                sample_ability(11, "completions", "gpt-5.4-mini"),
            ],
            &HashSet::from([11]),
        );

        assert_eq!(
            names,
            vec!["gpt-5.4".to_string(), "gpt-5.4-mini".to_string()]
        );
    }

    #[test]
    fn available_model_names_skips_inactive_channel_ids() {
        let names = available_model_names(
            vec![
                sample_ability(11, "chat", "gpt-5.4"),
                sample_ability(12, "responses", "gpt-5.4-mini"),
            ],
            &HashSet::from([11]),
        );

        assert_eq!(names, vec!["gpt-5.4".to_string()]);
    }

    #[test]
    fn account_is_available_for_model_listing_requires_api_key() {
        let now = chrono::Utc::now().fixed_offset();

        assert!(account_is_available_for_model_listing(
            &sample_account(11, serde_json::json!({"api_key": "sk-demo"})),
            now,
        ));
        assert!(!account_is_available_for_model_listing(
            &sample_account(11, serde_json::json!({})),
            now,
        ));
    }

    #[test]
    fn account_is_available_for_model_listing_rejects_blank_api_key() {
        let now = chrono::Utc::now().fixed_offset();

        assert!(!account_is_available_for_model_listing(
            &sample_account(11, serde_json::json!({"api_key": "   "})),
            now,
        ));
    }

    #[test]
    fn account_is_available_for_model_listing_rejects_expired_and_cooled_down_accounts() {
        let now = chrono::Utc::now().fixed_offset();

        let mut expired = sample_account(11, serde_json::json!({"api_key": "sk-demo"}));
        expired.expires_at = Some(now - chrono::Duration::minutes(1));
        assert!(!account_is_available_for_model_listing(&expired, now));

        let mut rate_limited = sample_account(11, serde_json::json!({"api_key": "sk-demo"}));
        rate_limited.rate_limited_until = Some(now + chrono::Duration::minutes(1));
        assert!(!account_is_available_for_model_listing(&rate_limited, now));

        let mut overloaded = sample_account(11, serde_json::json!({"api_key": "sk-demo"}));
        overloaded.overload_until = Some(now + chrono::Duration::minutes(1));
        assert!(!account_is_available_for_model_listing(&overloaded, now));
    }

    fn sample_ability(channel_id: i64, endpoint_scope: &str, model: &str) -> ability::Model {
        let now = chrono::Utc::now().fixed_offset();
        ability::Model {
            id: channel_id * 10,
            channel_group: "default".into(),
            endpoint_scope: endpoint_scope.into(),
            model: model.into(),
            channel_id,
            enabled: true,
            priority: 1,
            weight: 1,
            route_config: serde_json::json!({}),
            create_time: now,
            update_time: now,
        }
    }

    fn sample_account(channel_id: i64, credentials: serde_json::Value) -> channel_account::Model {
        use sea_orm::prelude::BigDecimal;

        let now = chrono::Utc::now().fixed_offset();
        channel_account::Model {
            id: channel_id * 10,
            channel_id,
            name: "demo-account".into(),
            credential_type: "api_key".into(),
            credentials,
            secret_ref: String::new(),
            status: channel_account::AccountStatus::Enabled,
            schedulable: true,
            priority: 1,
            weight: 1,
            rate_multiplier: BigDecimal::from(1),
            concurrency_limit: 1,
            quota_limit: BigDecimal::from(0),
            quota_used: BigDecimal::from(0),
            balance: BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            rate_limited_until: None,
            overload_until: None,
            expires_at: Some(now + chrono::Duration::minutes(10)),
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "tester".into(),
            create_time: now,
            update_by: "tester".into(),
            update_time: now,
        }
    }
}
