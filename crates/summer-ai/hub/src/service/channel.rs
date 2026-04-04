use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionError, TransactionTrait,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use summer_web::axum::http::HeaderMap;

use summer_ai_core::provider::{get_adapter, provider_scope_allowlist};
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::common::Message;
use summer_ai_core::types::responses::ResponsesResponse;
use summer_ai_model::dto::channel::{CreateChannelDto, QueryChannelDto, UpdateChannelDto};
use summer_ai_model::dto::channel_account::{
    CreateChannelAccountDto, QueryChannelAccountDto, UpdateChannelAccountDto,
};
use summer_ai_model::dto::endpoint_scope::normalize_endpoint_scope_list;
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_ai_model::entity::model_config;
use summer_ai_model::vo::channel::{ChannelDetailVo, ChannelListVo, ChannelTestVo};
use summer_ai_model::vo::channel_account::ChannelAccountVo;

mod health_logic;

use self::health_logic::{
    compute_failure_health_update, compute_relay_success_health_update,
    compute_test_success_health_update, relay_health_update_is_stale, relay_request_started_at,
    select_schedulable_account,
};

use crate::relay::channel_router::route_cache_version_key;
use crate::relay::http_client::UpstreamHttpClient;
use crate::service::route_health::RouteHealthService;
use crate::service::runtime_cache::RuntimeCacheService;

#[derive(Clone, Service)]
pub struct ChannelService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    http_client: UpstreamHttpClient,
    #[inject(component)]
    cache: RuntimeCacheService,
    #[inject(component)]
    route_health: RouteHealthService,
}

impl ChannelService {
    pub fn new(
        db: DbConn,
        http_client: UpstreamHttpClient,
        cache: RuntimeCacheService,
        route_health: RouteHealthService,
    ) -> Self {
        Self {
            db,
            http_client,
            cache,
            route_health,
        }
    }

    pub async fn list_channels(
        &self,
        query: QueryChannelDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelListVo>> {
        let page = channel::Entity::find()
            .filter(query)
            .order_by_desc(channel::Column::Priority)
            .order_by_desc(channel::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询渠道列表失败")?;

        Ok(page.map(ChannelListVo::from_model))
    }

    pub async fn get_channel(&self, id: i64) -> ApiResult<ChannelDetailVo> {
        Ok(ChannelDetailVo::from_model(
            self.find_channel_model(id).await?,
        ))
    }

    pub async fn create_channel(&self, dto: CreateChannelDto, operator: &str) -> ApiResult<()> {
        validate_provider_endpoint_scopes(
            dto.channel_type as i16,
            &dto.normalized_endpoint_scopes()
                .map_err(ApiErrors::BadRequest)?,
        )
        .map_err(ApiErrors::BadRequest)?;

        let model = dto
            .into_active_model(operator)
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建渠道失败")?;

        self.sync_abilities(&model).await?;
        self.invalidate_route_cache().await?;
        Ok(())
    }

    pub async fn update_channel(
        &self,
        id: i64,
        dto: UpdateChannelDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_channel_model(id).await?;
        let final_channel_type = model.channel_type as i16;
        let endpoint_scopes = dto
            .endpoint_scopes
            .as_ref()
            .unwrap_or(&model.endpoint_scopes);
        let normalized_scopes = normalize_endpoint_scope_list(endpoint_scopes, "endpointScopes")
            .map_err(ApiErrors::BadRequest)?;
        validate_provider_endpoint_scopes(final_channel_type, &normalized_scopes)
            .map_err(ApiErrors::BadRequest)?;

        let mut active: channel::ActiveModel = model.into();
        dto.apply_to(&mut active, operator)
            .map_err(ApiErrors::BadRequest)?;

        let model = active.update(&self.db).await.context("更新渠道失败")?;
        self.sync_abilities(&model).await?;
        self.invalidate_route_cache().await?;
        Ok(())
    }

    pub async fn delete_channel(&self, id: i64, operator: &str) -> ApiResult<()> {
        let mut active: channel::ActiveModel = self.find_channel_model(id).await?.into();
        let now = chrono::Utc::now().fixed_offset();
        active.status = Set(ChannelStatus::Archived);
        active.deleted_at = Set(Some(now));
        active.update_by = Set(operator.to_string());
        active.update(&self.db).await.context("删除渠道失败")?;

        ability::Entity::delete_many()
            .filter(ability::Column::ChannelId.eq(id))
            .exec(&self.db)
            .await
            .context("删除渠道能力失败")?;

        self.invalidate_route_cache().await?;
        Ok(())
    }

    pub async fn test_channel(
        &self,
        id: i64,
        endpoint_scope: Option<String>,
    ) -> ApiResult<ChannelTestVo> {
        let channel = self.find_channel_model(id).await?;
        resolve_probe_endpoint_scope(
            channel.channel_type as i16,
            &channel.endpoint_scopes,
            endpoint_scope.as_deref(),
        )?;
        let account = self
            .get_schedulable_account(id)
            .await?
            .ok_or_else(|| ApiErrors::BadRequest("渠道下没有可调度账号".to_string()))?;

        self.run_channel_probe(channel, account, endpoint_scope.as_deref())
            .await
    }

    pub async fn recover_auto_disabled_channels(&self) -> ApiResult<()> {
        let now = chrono::Utc::now().fixed_offset();
        // Base cooldown: 5 minutes since last error.
        let base_cooldown = chrono::Duration::minutes(5);

        let channels = channel::Entity::find()
            .filter(channel::Column::Status.eq(ChannelStatus::AutoDisabled))
            .filter(channel::Column::DeletedAt.is_null())
            .filter(
                sea_orm::Condition::any()
                    .add(channel::Column::LastErrorAt.is_null())
                    .add(channel::Column::LastErrorAt.lt(now - base_cooldown)),
            )
            .order_by_asc(channel::Column::Id)
            .all(&self.db)
            .await
            .context("查询自动禁用渠道失败")
            .map_err(ApiErrors::Internal)?;

        for channel in channels {
            // Exponential backoff based on failure_streak:
            //   streak 0-1 → 5min, 2 → 10min, 3 → 20min, 4 → 40min, 5+ → 60min (cap)
            let backoff_minutes = (5_i64 * (1 << channel.failure_streak.min(4) as u32)).min(60);
            if let Some(last_error) = channel.last_error_at {
                let backoff_cutoff = last_error + chrono::Duration::minutes(backoff_minutes);
                if now < backoff_cutoff {
                    continue;
                }
            }
            if let Err(error) = resolve_probe_endpoint_scope(
                channel.channel_type as i16,
                &channel.endpoint_scopes,
                None,
            ) {
                tracing::warn!(
                    "skip auto recovery for channel {} because no probeable endpoint scope is configured: {}",
                    channel.id,
                    error
                );
                continue;
            }

            let Some(account) = self.get_schedulable_account(channel.id).await? else {
                tracing::warn!(
                    "skip auto recovery for channel {} because no schedulable account is available",
                    channel.id
                );
                continue;
            };

            match self.run_channel_probe(channel.clone(), account, None).await {
                Ok(result) if result.success => {
                    tracing::info!(
                        "auto recovery succeeded for channel {} in {} ms",
                        channel.id,
                        result.elapsed_ms
                    );
                }
                Ok(result) => {
                    tracing::warn!(
                        "auto recovery probe failed for channel {}: {}",
                        channel.id,
                        result.message
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        "auto recovery probe errored for channel {}: {}",
                        channel.id,
                        error
                    );
                }
            }
        }

        Ok(())
    }

    async fn run_channel_probe(
        &self,
        channel: channel::Model,
        account: channel_account::Model,
        endpoint_scope: Option<&str>,
    ) -> ApiResult<ChannelTestVo> {
        let api_key = Self::extract_api_key(&account.credentials);
        if api_key.is_empty() {
            return Err(ApiErrors::BadRequest(
                "渠道账号缺少 api_key 凭证".to_string(),
            ));
        }

        let probe_scope = resolve_probe_endpoint_scope(
            channel.channel_type as i16,
            &channel.endpoint_scopes,
            endpoint_scope,
        )?;
        let configured_models = self.json_string_list(&channel.models);
        let model_supported_endpoints = self
            .load_model_supported_endpoints(&configured_models)
            .await?;
        let test_model = self
            .pick_test_model(&channel, Some(probe_scope), &model_supported_endpoints)
            .ok_or_else(|| ApiErrors::BadRequest("渠道未配置 test_model 或 models".to_string()))?;
        let actual_model = resolve_probe_model(&test_model, &channel.model_mapping);

        let adapter = get_adapter(channel.channel_type as i16);
        let request_builder = match probe_scope {
            "chat" => {
                let request = ChatCompletionRequest {
                    model: test_model.clone(),
                    messages: vec![Message {
                        role: "user".into(),
                        content: serde_json::Value::String("ping".into()),
                        name: None,
                        tool_calls: None,
                        tool_call_id: None,
                    }],
                    stream: false,
                    temperature: None,
                    max_tokens: Some(8),
                    top_p: None,
                    frequency_penalty: None,
                    presence_penalty: None,
                    stop: None,
                    tools: None,
                    tool_choice: None,
                    response_format: None,
                    stream_options: None,
                    extra: serde_json::Map::new(),
                };
                adapter.build_request(
                    self.http_client.client(),
                    &channel.base_url,
                    &api_key,
                    &request,
                    &actual_model,
                )
            }
            "responses" => {
                let request = serde_json::json!({
                    "model": test_model,
                    "input": "ping",
                    "stream": false,
                });
                adapter.build_responses_request(
                    self.http_client.client(),
                    &channel.base_url,
                    &api_key,
                    &request,
                    &actual_model,
                )
            }
            "embeddings" => {
                let request = serde_json::json!({
                    "model": test_model,
                    "input": "ping",
                });
                adapter.build_embeddings_request(
                    self.http_client.client(),
                    &channel.base_url,
                    &api_key,
                    &request,
                    &actual_model,
                )
            }
            _ => Err(anyhow::anyhow!(
                "channel test does not support endpoint scope: {probe_scope}"
            )),
        }
        .map_err(ApiErrors::Internal)?;

        let start = std::time::Instant::now();
        match request_builder.send().await {
            Ok(response) => {
                let elapsed = start.elapsed().as_millis() as i64;
                let status_code = response.status().as_u16() as i32;

                if response.status().is_success() {
                    let body = response
                        .bytes()
                        .await
                        .context("读取渠道测速响应失败")
                        .map_err(ApiErrors::Internal)?;
                    if let Err(error) = validate_probe_success_body(
                        channel.channel_type as i16,
                        probe_scope,
                        &actual_model,
                        body,
                    ) {
                        let message = format!("failed to parse channel test response: {error}");
                        self.write_test_failure(
                            channel.clone(),
                            account.clone(),
                            elapsed,
                            0,
                            &message,
                        )
                        .await?;

                        return Ok(ChannelTestVo {
                            success: false,
                            status_code: 0,
                            elapsed_ms: elapsed,
                            message,
                        });
                    }

                    self.write_test_success(channel.clone(), account.clone(), elapsed)
                        .await?;

                    Ok(ChannelTestVo {
                        success: true,
                        status_code,
                        elapsed_ms: elapsed,
                        message: "channel test succeeded".into(),
                    })
                } else {
                    let status = response.status();
                    let headers = response.headers().clone();
                    let body = response.bytes().await.unwrap_or_default();
                    let message = provider_probe_failure_message(
                        channel.channel_type as i16,
                        status,
                        &headers,
                        &body,
                    );
                    self.write_test_failure(
                        channel.clone(),
                        account.clone(),
                        elapsed,
                        status_code,
                        &message,
                    )
                    .await?;

                    Ok(ChannelTestVo {
                        success: false,
                        status_code,
                        elapsed_ms: elapsed,
                        message,
                    })
                }
            }
            Err(error) => {
                let elapsed = start.elapsed().as_millis() as i64;
                self.write_test_failure(channel, account, elapsed, 0, &error.to_string())
                    .await?;

                Ok(ChannelTestVo {
                    success: false,
                    status_code: 0,
                    elapsed_ms: elapsed,
                    message: error.to_string(),
                })
            }
        }
    }

    pub async fn list_accounts(
        &self,
        query: QueryChannelAccountDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelAccountVo>> {
        let page = channel_account::Entity::find()
            .filter(query)
            .order_by_desc(channel_account::Column::Priority)
            .order_by_desc(channel_account::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询渠道账号列表失败")?;

        Ok(page.map(ChannelAccountVo::from_model))
    }

    pub async fn create_account(
        &self,
        dto: CreateChannelAccountDto,
        operator: &str,
    ) -> ApiResult<()> {
        self.find_channel_model(dto.channel_id).await?;
        dto.into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建渠道账号失败")?;
        self.invalidate_route_cache().await?;
        Ok(())
    }

    pub async fn update_account(
        &self,
        id: i64,
        dto: UpdateChannelAccountDto,
        operator: &str,
    ) -> ApiResult<()> {
        let mut active: channel_account::ActiveModel = self.find_account_model(id).await?.into();
        dto.apply_to(&mut active, operator);
        active.update(&self.db).await.context("更新渠道账号失败")?;
        self.invalidate_route_cache().await?;
        Ok(())
    }

    pub async fn delete_account(&self, id: i64, operator: &str) -> ApiResult<()> {
        let mut active: channel_account::ActiveModel = self.find_account_model(id).await?.into();
        let now = chrono::Utc::now().fixed_offset();
        active.status = Set(AccountStatus::Disabled);
        active.schedulable = Set(false);
        active.deleted_at = Set(Some(now));
        active.update_by = Set(operator.to_string());
        active.update(&self.db).await.context("删除渠道账号失败")?;
        self.invalidate_route_cache().await?;
        Ok(())
    }

    pub async fn get_by_id(&self, id: i64) -> ApiResult<Option<channel::Model>> {
        channel::Entity::find_by_id(id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道失败")
            .map_err(ApiErrors::Internal)
    }

    pub async fn get_schedulable_account(
        &self,
        channel_id: i64,
    ) -> ApiResult<Option<channel_account::Model>> {
        channel_account::Entity::find()
            .filter(channel_account::Column::ChannelId.eq(channel_id))
            .filter(channel_account::Column::Status.eq(AccountStatus::Enabled))
            .filter(channel_account::Column::Schedulable.eq(true))
            .filter(channel_account::Column::DeletedAt.is_null())
            .order_by_desc(channel_account::Column::Priority)
            .order_by_desc(channel_account::Column::Id)
            .all(&self.db)
            .await
            .context("查询渠道账号失败")
            .map_err(ApiErrors::Internal)
            .map(select_schedulable_account)
    }

    pub fn extract_api_key(credentials: &serde_json::Value) -> String {
        credentials
            .get("api_key")
            .or_else(|| credentials.get("apiKey"))
            .or_else(|| credentials.get("key"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .to_string()
    }

    pub async fn record_relay_success(
        &self,
        channel_id: i64,
        account_id: i64,
        elapsed_ms: i64,
    ) -> ApiResult<()> {
        let this = self.clone();
        let invalidate_route_cache = self
            .db
            .transaction(move |txn| {
                let this = this.clone();
                Box::pin(async move {
                    let channel_model = channel::Entity::find_by_id(channel_id)
                        .filter(channel::Column::DeletedAt.is_null())
                        .lock_exclusive()
                        .one(txn)
                        .await
                        .context("查询渠道详情失败")
                        .map_err(ApiErrors::Internal)?
                        .ok_or_else(|| ApiErrors::NotFound("渠道不存在".to_string()))?;
                    let account_model = channel_account::Entity::find_by_id(account_id)
                        .filter(channel_account::Column::DeletedAt.is_null())
                        .lock_exclusive()
                        .one(txn)
                        .await
                        .context("查询渠道账号详情失败")
                        .map_err(ApiErrors::Internal)?
                        .ok_or_else(|| ApiErrors::NotFound("渠道账号不存在".to_string()))?;

                    this.write_relay_success_with_conn(
                        txn,
                        channel_model,
                        account_model,
                        elapsed_ms,
                    )
                    .await
                })
            })
            .await
            .map_err(map_relay_health_transaction_error)?;
        let route_health_changed = match self
            .route_health
            .record_relay_success(channel_id, account_id)
            .await
        {
            Ok(changed) => changed,
            Err(error) => {
                tracing::warn!("failed to record route health relay success: {error}");
                false
            }
        };

        if invalidate_route_cache || route_health_changed {
            self.invalidate_route_cache().await?;
        }
        Ok(())
    }

    pub async fn record_relay_failure(
        &self,
        channel_id: i64,
        account_id: i64,
        elapsed_ms: i64,
        status_code: i32,
        message: &str,
    ) -> ApiResult<()> {
        let message = message.to_string();
        let this = self.clone();
        let invalidate_route_cache = self
            .db
            .transaction(move |txn| {
                let this = this.clone();
                Box::pin(async move {
                    let channel_model = channel::Entity::find_by_id(channel_id)
                        .filter(channel::Column::DeletedAt.is_null())
                        .lock_exclusive()
                        .one(txn)
                        .await
                        .context("查询渠道详情失败")
                        .map_err(ApiErrors::Internal)?
                        .ok_or_else(|| ApiErrors::NotFound("渠道不存在".to_string()))?;
                    let account_model = channel_account::Entity::find_by_id(account_id)
                        .filter(channel_account::Column::DeletedAt.is_null())
                        .lock_exclusive()
                        .one(txn)
                        .await
                        .context("查询渠道账号详情失败")
                        .map_err(ApiErrors::Internal)?
                        .ok_or_else(|| ApiErrors::NotFound("渠道账号不存在".to_string()))?;

                    this.write_relay_failure_with_conn(
                        txn,
                        channel_model,
                        account_model,
                        elapsed_ms,
                        status_code,
                        &message,
                    )
                    .await
                })
            })
            .await
            .map_err(map_relay_health_transaction_error)?;
        let route_health_changed = match self
            .route_health
            .record_relay_failure(channel_id, account_id, status_code)
            .await
        {
            Ok(changed) => changed,
            Err(error) => {
                tracing::warn!("failed to record route health relay failure: {error}");
                false
            }
        };

        if invalidate_route_cache || route_health_changed {
            self.invalidate_route_cache().await?;
        }
        Ok(())
    }

    pub fn record_relay_failure_async(
        &self,
        channel_id: i64,
        account_id: i64,
        elapsed_ms: i64,
        status_code: i32,
        message: impl Into<String>,
    ) {
        let this = self.clone();
        let message = message.into();
        tokio::spawn(async move {
            if let Err(error) = this
                .record_relay_failure(channel_id, account_id, elapsed_ms, status_code, &message)
                .await
            {
                tracing::warn!("failed to record relay failure health state: {error}");
            }
        });
    }

    async fn sync_abilities(&self, model: &channel::Model) -> ApiResult<()> {
        ability::Entity::delete_many()
            .filter(ability::Column::ChannelId.eq(model.id))
            .exec(&self.db)
            .await
            .context("同步渠道能力前清理旧记录失败")?;

        if model.deleted_at.is_some() {
            return Ok(());
        }

        let configured_models = self.json_string_list(&model.models);
        let model_supported_endpoints = self
            .load_model_supported_endpoints(&configured_models)
            .await?;
        let abilities = self.build_abilities(model, &model_supported_endpoints);
        if !abilities.is_empty() {
            ability::Entity::insert_many(abilities)
                .exec(&self.db)
                .await
                .context("同步渠道能力失败")?;
        }

        Ok(())
    }

    pub async fn resync_abilities_for_model_name(&self, model_name: &str) -> ApiResult<()> {
        if model_name.trim().is_empty() {
            return Ok(());
        }

        let channels = channel::Entity::find()
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("查询待同步渠道失败")
            .map_err(ApiErrors::Internal)?;

        let mut touched = false;
        for channel_model in channels {
            let configured_models = self.json_string_list(&channel_model.models);
            if configured_models
                .iter()
                .any(|configured| configured == model_name)
            {
                self.sync_abilities(&channel_model).await?;
                touched = true;
            }
        }

        if touched {
            self.invalidate_route_cache().await?;
        }

        Ok(())
    }

    fn build_abilities(
        &self,
        model: &channel::Model,
        model_supported_endpoints: &std::collections::HashMap<String, Vec<String>>,
    ) -> Vec<ability::ActiveModel> {
        let models = self.json_string_list(&model.models);
        let scopes = effective_channel_endpoint_scopes(
            model.channel_type as i16,
            self.json_string_list(&model.endpoint_scopes),
        );

        build_model_endpoint_scope_pairs(models, scopes, model_supported_endpoints)
            .into_iter()
            .map(|(item, scope)| ability::ActiveModel {
                channel_group: Set(model.channel_group.clone()),
                endpoint_scope: Set(scope),
                model: Set(item),
                channel_id: Set(model.id),
                enabled: Set(model.status == ChannelStatus::Enabled),
                priority: Set(model.priority),
                weight: Set(model.weight),
                route_config: Set(serde_json::json!({})),
                ..Default::default()
            })
            .collect()
    }

    fn json_string_list(&self, value: &serde_json::Value) -> Vec<String> {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn load_model_supported_endpoints(
        &self,
        models: &[String],
    ) -> ApiResult<std::collections::HashMap<String, Vec<String>>> {
        if models.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let configs = model_config::Entity::find()
            .filter(model_config::Column::ModelName.is_in(models.to_vec()))
            .all(&self.db)
            .await
            .context("查询模型支持端点失败")
            .map_err(ApiErrors::Internal)?;

        Ok(configs
            .into_iter()
            .map(|config| {
                (
                    config.model_name,
                    self.json_string_list(&config.supported_endpoints),
                )
            })
            .collect())
    }

    fn pick_test_model(
        &self,
        channel: &channel::Model,
        probe_scope: Option<&str>,
        model_supported_endpoints: &std::collections::HashMap<String, Vec<String>>,
    ) -> Option<String> {
        select_probe_model(
            &channel.test_model,
            self.json_string_list(&channel.models),
            probe_scope,
            model_supported_endpoints,
        )
    }

    async fn find_channel_model(&self, id: i64) -> ApiResult<channel::Model> {
        channel::Entity::find_by_id(id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("渠道不存在".to_string()))
    }

    async fn find_account_model(&self, id: i64) -> ApiResult<channel_account::Model> {
        channel_account::Entity::find_by_id(id)
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道账号详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("渠道账号不存在".to_string()))
    }

    async fn write_test_success(
        &self,
        channel_model: channel::Model,
        account_model: channel_account::Model,
        elapsed_ms: i64,
    ) -> ApiResult<()> {
        let now = chrono::Utc::now().fixed_offset();
        let health_update = compute_test_success_health_update(
            channel_model.status,
            account_model.rate_limited_until,
            account_model.overload_until,
            now,
        );
        let should_invalidate_route_cache = health_update.invalidate_route_cache
            || channel_model.failure_streak > 0
            || account_model.failure_streak > 0
            || channel_model.last_health_status != 1;

        let mut channel_active: channel::ActiveModel = channel_model.into();
        channel_active.response_time = Set(elapsed_ms as i32);
        channel_active.failure_streak = Set(0);
        channel_active.last_used_at = Set(Some(now));
        channel_active.last_health_status = Set(1);
        channel_active.last_error_at = Set(None);
        channel_active.last_error_code = Set(String::new());
        channel_active.last_error_message = Set(None);
        channel_active.status = Set(health_update.next_channel_status);
        channel_active
            .update(&self.db)
            .await
            .context("更新渠道测速结果失败")?;

        let mut account_active: channel_account::ActiveModel = account_model.into();
        account_active.response_time = Set(elapsed_ms as i32);
        account_active.failure_streak = Set(0);
        account_active.last_used_at = Set(Some(now));
        account_active.last_error_at = Set(None);
        account_active.last_error_code = Set(String::new());
        account_active.last_error_message = Set(None);
        account_active.rate_limited_until = Set(health_update.next_rate_limited_until);
        account_active.overload_until = Set(health_update.next_overload_until);
        account_active.test_time = Set(Some(now));
        account_active
            .update(&self.db)
            .await
            .context("更新渠道账号测速结果失败")?;

        if should_invalidate_route_cache {
            self.invalidate_route_cache().await?;
        }
        Ok(())
    }

    async fn write_test_failure(
        &self,
        channel_model: channel::Model,
        account_model: channel_account::Model,
        elapsed_ms: i64,
        status_code: i32,
        message: &str,
    ) -> ApiResult<()> {
        let now = chrono::Utc::now().fixed_offset();
        let error_code = if status_code == 0 {
            "request_error".to_string()
        } else {
            status_code.to_string()
        };
        let health_update = compute_failure_health_update(
            status_code,
            channel_model.status,
            account_model.status,
            account_model.schedulable,
            channel_model.failure_streak,
            account_model.failure_streak,
            channel_model.auto_ban,
            account_model.rate_limited_until,
            account_model.overload_until,
            now,
        );

        let mut channel_active: channel::ActiveModel = channel_model.into();
        channel_active.response_time = Set(elapsed_ms as i32);
        channel_active.failure_streak = Set(health_update.next_channel_failure_streak);
        channel_active.last_health_status = Set(health_update.next_health_status);
        channel_active.last_error_at = Set(Some(now));
        channel_active.last_error_code = Set(error_code.clone());
        channel_active.last_error_message = Set(Some(message.to_string()));
        channel_active.status = Set(health_update.next_channel_status);
        channel_active
            .update(&self.db)
            .await
            .context("更新渠道失败结果失败")?;

        let mut account_active: channel_account::ActiveModel = account_model.into();
        account_active.response_time = Set(elapsed_ms as i32);
        account_active.status = Set(health_update.next_account_status);
        account_active.schedulable = Set(health_update.next_account_schedulable);
        account_active.failure_streak = Set(health_update.next_account_failure_streak);
        account_active.last_error_at = Set(Some(now));
        account_active.last_error_code = Set(error_code);
        account_active.last_error_message = Set(Some(message.to_string()));
        account_active.rate_limited_until = Set(health_update.cooldown.rate_limited_until);
        account_active.overload_until = Set(health_update.cooldown.overload_until);
        account_active.test_time = Set(Some(now));
        account_active
            .update(&self.db)
            .await
            .context("更新渠道账号失败结果失败")?;

        if health_update.invalidate_route_cache {
            self.invalidate_route_cache().await?;
        }
        Ok(())
    }

    async fn write_relay_success_with_conn<C: ConnectionTrait>(
        &self,
        conn: &C,
        channel_model: channel::Model,
        account_model: channel_account::Model,
        elapsed_ms: i64,
    ) -> ApiResult<bool> {
        let now = chrono::Utc::now().fixed_offset();
        let request_started_at = relay_request_started_at(now, elapsed_ms);
        if relay_health_update_is_stale(
            channel_model.last_error_at,
            account_model.last_error_at,
            request_started_at,
        ) {
            return Ok(false);
        }
        let health_update = compute_relay_success_health_update(
            channel_model.status,
            account_model.rate_limited_until,
            account_model.overload_until,
            now,
        );
        let should_invalidate_route_cache = health_update.invalidate_route_cache
            || channel_model.failure_streak > 0
            || account_model.failure_streak > 0
            || channel_model.last_health_status != 1;

        let mut channel_active: channel::ActiveModel = channel_model.into();
        channel_active.response_time = Set(elapsed_ms as i32);
        channel_active.failure_streak = Set(0);
        channel_active.last_used_at = Set(Some(now));
        channel_active.last_health_status = Set(1);
        channel_active.last_error_at = Set(None);
        channel_active.last_error_code = Set(String::new());
        channel_active.last_error_message = Set(None);
        channel_active.status = Set(health_update.next_channel_status);
        channel_active
            .update(conn)
            .await
            .context("更新渠道转发成功结果失败")?;

        let mut account_active: channel_account::ActiveModel = account_model.into();
        account_active.response_time = Set(elapsed_ms as i32);
        account_active.failure_streak = Set(0);
        account_active.last_used_at = Set(Some(now));
        account_active.last_error_at = Set(None);
        account_active.last_error_code = Set(String::new());
        account_active.last_error_message = Set(None);
        account_active.rate_limited_until = Set(health_update.next_rate_limited_until);
        account_active.overload_until = Set(health_update.next_overload_until);
        account_active
            .update(conn)
            .await
            .context("更新渠道账号转发成功结果失败")?;

        Ok(should_invalidate_route_cache)
    }

    async fn write_relay_failure_with_conn<C: ConnectionTrait>(
        &self,
        conn: &C,
        channel_model: channel::Model,
        account_model: channel_account::Model,
        elapsed_ms: i64,
        status_code: i32,
        message: &str,
    ) -> ApiResult<bool> {
        let now = chrono::Utc::now().fixed_offset();
        let request_started_at = relay_request_started_at(now, elapsed_ms);
        if relay_health_update_is_stale(
            channel_model.last_error_at,
            account_model.last_error_at,
            request_started_at,
        ) {
            return Ok(false);
        }
        let error_code = if status_code == 0 {
            "request_error".to_string()
        } else {
            status_code.to_string()
        };
        let health_update = compute_failure_health_update(
            status_code,
            channel_model.status,
            account_model.status,
            account_model.schedulable,
            channel_model.failure_streak,
            account_model.failure_streak,
            channel_model.auto_ban,
            account_model.rate_limited_until,
            account_model.overload_until,
            now,
        );

        let mut channel_active: channel::ActiveModel = channel_model.into();
        channel_active.response_time = Set(elapsed_ms as i32);
        channel_active.failure_streak = Set(health_update.next_channel_failure_streak);
        channel_active.last_health_status = Set(health_update.next_health_status);
        channel_active.last_error_at = Set(Some(now));
        channel_active.last_error_code = Set(error_code.clone());
        channel_active.last_error_message = Set(Some(message.to_string()));
        channel_active.status = Set(health_update.next_channel_status);
        channel_active
            .update(conn)
            .await
            .context("更新渠道转发失败结果失败")?;

        let mut account_active: channel_account::ActiveModel = account_model.into();
        account_active.response_time = Set(elapsed_ms as i32);
        account_active.status = Set(health_update.next_account_status);
        account_active.schedulable = Set(health_update.next_account_schedulable);
        account_active.failure_streak = Set(health_update.next_account_failure_streak);
        account_active.last_error_at = Set(Some(now));
        account_active.last_error_code = Set(error_code);
        account_active.last_error_message = Set(Some(message.to_string()));
        account_active.rate_limited_until = Set(health_update.cooldown.rate_limited_until);
        account_active.overload_until = Set(health_update.cooldown.overload_until);
        account_active
            .update(conn)
            .await
            .context("更新渠道账号转发失败结果失败")?;

        Ok(health_update.invalidate_route_cache)
    }

    async fn invalidate_route_cache(&self) -> ApiResult<()> {
        let _ = self.cache.incr(route_cache_version_key()).await?;
        Ok(())
    }
}

fn map_relay_health_transaction_error(error: TransactionError<ApiErrors>) -> ApiErrors {
    match error {
        TransactionError::Connection(error) => ApiErrors::Internal(error.into()),
        TransactionError::Transaction(error) => error,
    }
}

fn provider_probe_failure_message(
    channel_type: i16,
    status: summer_web::axum::http::StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> String {
    let info = get_adapter(channel_type).parse_error(status.into(), headers, body);
    if info.message.is_empty() {
        String::from_utf8_lossy(body).trim().to_string()
    } else {
        info.message
    }
}

fn validate_probe_success_body(
    channel_type: i16,
    probe_scope: &str,
    model: &str,
    body: bytes::Bytes,
) -> ApiResult<()> {
    match probe_scope {
        "chat" => get_adapter(channel_type)
            .parse_response(body, model)
            .map(|_| ())
            .map_err(ApiErrors::Internal),
        "responses" => serde_json::from_slice::<ResponsesResponse>(&body)
            .map(|_| ())
            .map_err(|error| ApiErrors::Internal(error.into())),
        "embeddings" => get_adapter(channel_type)
            .parse_embeddings_response(body, model, 0)
            .map(|_| ())
            .map_err(ApiErrors::Internal),
        _ => Err(ApiErrors::BadRequest(format!(
            "channel test does not support endpoint scope: {probe_scope}"
        ))),
    }
}

fn build_model_endpoint_scope_pairs(
    models: Vec<String>,
    scopes: Vec<String>,
    model_supported_endpoints: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<(String, String)> {
    models
        .into_iter()
        .flat_map(|model| {
            let allowed_scopes = model_supported_endpoints.get(&model);
            scopes
                .iter()
                .filter(move |scope| {
                    allowed_scopes.is_none_or(|supported| supported.contains(*scope))
                })
                .cloned()
                .map(move |scope| (model.clone(), scope))
        })
        .collect()
}

fn select_probe_model(
    configured_test_model: &str,
    models: Vec<String>,
    probe_scope: Option<&str>,
    model_supported_endpoints: &std::collections::HashMap<String, Vec<String>>,
) -> Option<String> {
    if !configured_test_model.trim().is_empty() {
        return Some(configured_test_model.to_string());
    }

    if let Some(probe_scope) = probe_scope
        && let Some(model) = models.iter().find(|model| {
            model_supported_endpoints
                .get(*model)
                .is_some_and(|supported| supported.iter().any(|scope| scope == probe_scope))
        })
    {
        return Some(model.clone());
    }

    models.into_iter().next()
}

fn resolve_probe_model(test_model: &str, model_mapping: &serde_json::Value) -> String {
    model_mapping
        .get(test_model)
        .and_then(|value| value.as_str())
        .unwrap_or(test_model)
        .to_string()
}

fn effective_channel_endpoint_scopes(
    channel_type: i16,
    configured_scopes: Vec<String>,
) -> Vec<String> {
    let scopes = if configured_scopes.is_empty() {
        vec!["chat".to_string()]
    } else {
        configured_scopes
    };

    if let Some(allowlist) = provider_scope_allowlist(channel_type) {
        scopes
            .into_iter()
            .filter(|scope| allowlist.contains(&scope.as_str()))
            .collect()
    } else {
        scopes
    }
}

fn validate_provider_endpoint_scopes(
    channel_type: i16,
    configured_scopes: &[String],
) -> Result<(), String> {
    let scopes = if configured_scopes.is_empty() {
        vec!["chat".to_string()]
    } else {
        configured_scopes.to_vec()
    };

    let Some(allowlist) = provider_scope_allowlist(channel_type) else {
        return Ok(());
    };

    let unsupported: Vec<String> = scopes
        .into_iter()
        .filter(|scope| !allowlist.contains(&scope.as_str()))
        .collect();
    if unsupported.is_empty() {
        return Ok(());
    }

    Err(format!(
        "channel type {channel_type} does not support endpoint scopes: {}",
        unsupported.join(", ")
    ))
}

fn pick_probe_endpoint_scope(
    channel_type: i16,
    configured_scopes: &serde_json::Value,
) -> Option<&'static str> {
    let scopes = probeable_endpoint_scopes(channel_type, configured_scopes);

    ["chat", "responses", "embeddings"]
        .into_iter()
        .find(|candidate| scopes.iter().any(|scope| scope == candidate))
}

fn resolve_probe_endpoint_scope(
    channel_type: i16,
    configured_scopes: &serde_json::Value,
    requested_scope: Option<&str>,
) -> ApiResult<&'static str> {
    let scopes = probeable_endpoint_scopes(channel_type, configured_scopes);
    if let Some(requested_scope) = requested_scope {
        let requested_scope = requested_scope.trim().to_ascii_lowercase();
        let selected = match requested_scope.as_str() {
            "chat" => "chat",
            "responses" => "responses",
            "embeddings" => "embeddings",
            _ => {
                return Err(ApiErrors::BadRequest(format!(
                    "channel test does not support endpoint scope: {requested_scope}"
                )));
            }
        };

        if scopes.iter().any(|scope| scope == selected) {
            return Ok(selected);
        }

        return Err(ApiErrors::BadRequest(format!(
            "endpoint scope is not enabled for channel test: {selected}"
        )));
    }

    pick_probe_endpoint_scope(channel_type, configured_scopes).ok_or_else(|| {
        ApiErrors::BadRequest(
            "channel test requires one of these endpoint scopes: chat, responses, embeddings"
                .to_string(),
        )
    })
}

fn probeable_endpoint_scopes(
    channel_type: i16,
    configured_scopes: &serde_json::Value,
) -> Vec<String> {
    effective_channel_endpoint_scopes(
        channel_type,
        configured_scopes
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
    )
    .into_iter()
    .filter(|scope| matches!(scope.as_str(), "chat" | "responses" | "embeddings"))
    .collect()
}

#[cfg(test)]
mod tests;
