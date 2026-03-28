use std::collections::BTreeSet;
use std::time::Instant;

use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use summer::plugin::Service;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_core::provider::get_adapter;
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::common::Message;
use summer_ai_model::dto::channel::{CreateChannelDto, QueryChannelDto, UpdateChannelDto};
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_ai_model::vo::channel::{ChannelDetailVo, ChannelTestVo, ChannelVo};
use summer_common::error::{ApiErrors, ApiResult};

/// 渠道查询相关的简单封装
#[derive(Clone, Service)]
pub struct ChannelService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelService {
    pub async fn list_channels(
        &self,
        query: QueryChannelDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelVo>> {
        let page = channel::Entity::find()
            .filter(query)
            .order_by_desc(channel::Column::Priority)
            .order_by_desc(channel::Column::Weight)
            .order_by_desc(channel::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("failed to list channels")
            .map_err(ApiErrors::Internal)?;

        Ok(page.map(ChannelVo::from_model))
    }

    pub async fn get_channel(&self, id: i64) -> ApiResult<ChannelDetailVo> {
        let channel = Self::find_channel(&self.db, id).await?;
        Ok(ChannelDetailVo::from_model(channel))
    }

    pub async fn create_channel(
        &self,
        dto: CreateChannelDto,
        operator: &str,
    ) -> ApiResult<ChannelDetailVo> {
        let operator = operator.to_string();
        self.db
            .transaction::<_, ChannelDetailVo, ApiErrors>(|txn| {
                let operator = operator.clone();
                Box::pin(async move {
                    let channel = dto
                        .into_active_model(&operator)
                        .insert(txn)
                        .await
                        .context("failed to create channel")
                        .map_err(ApiErrors::Internal)?;

                    Self::replace_channel_abilities(txn, &channel).await?;
                    Ok(ChannelDetailVo::from_model(channel))
                })
            })
            .await
            .map_err(ApiErrors::from)
    }

    pub async fn update_channel(
        &self,
        id: i64,
        dto: UpdateChannelDto,
        operator: &str,
    ) -> ApiResult<ChannelDetailVo> {
        let operator = operator.to_string();
        self.db
            .transaction::<_, ChannelDetailVo, ApiErrors>(|txn| {
                let operator = operator.clone();
                let dto = dto.clone();
                Box::pin(async move {
                    let channel = Self::find_channel(txn, id).await?;
                    let mut active: channel::ActiveModel = channel.into();
                    dto.apply_to(&mut active, &operator);
                    let updated = active
                        .update(txn)
                        .await
                        .context("failed to update channel")
                        .map_err(ApiErrors::Internal)?;

                    Self::replace_channel_abilities(txn, &updated).await?;
                    Ok(ChannelDetailVo::from_model(updated))
                })
            })
            .await
            .map_err(ApiErrors::from)
    }

    pub async fn delete_channel(&self, id: i64, operator: &str) -> ApiResult<()> {
        let operator = operator.to_string();
        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                Box::pin(async move {
                    let channel = Self::find_channel(txn, id).await?;
                    let mut active: channel::ActiveModel = channel.into();
                    active.status = Set(ChannelStatus::Archived);
                    active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
                    active.update_by = Set(operator);
                    active
                        .update(txn)
                        .await
                        .context("failed to delete channel")
                        .map_err(ApiErrors::Internal)?;

                    ability::Entity::delete_many()
                        .filter(ability::Column::ChannelId.eq(id))
                        .exec(txn)
                        .await
                        .context("failed to delete channel abilities")
                        .map_err(ApiErrors::Internal)?;
                    Ok(())
                })
            })
            .await
            .map_err(ApiErrors::from)
    }

    /// 根据 ID 获取渠道
    pub async fn get_by_id(&self, id: i64) -> ApiResult<Option<channel::Model>> {
        channel::Entity::find_by_id(id)
            .filter(channel::Column::DeletedAt.is_null())
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

    pub async fn test_channel(
        &self,
        id: i64,
        client: &reqwest::Client,
    ) -> ApiResult<ChannelTestVo> {
        let channel = Self::find_channel(&self.db, id).await?;
        let account = self
            .get_schedulable_account(id)
            .await?
            .ok_or_else(|| ApiErrors::NotFound("schedulable channel account not found".into()))?;

        let requested_model = resolve_test_model(&channel, &account)
            .ok_or_else(|| ApiErrors::BadRequest("channel test model is not configured".into()))?;
        let actual_model = channel
            .model_mapping
            .get(&requested_model)
            .and_then(|value| value.as_str())
            .unwrap_or(&requested_model)
            .to_string();
        let api_key = Self::extract_api_key(&account.credentials);
        if api_key.is_empty() {
            return Err(ApiErrors::BadRequest(
                "channel account API key is empty".into(),
            ));
        }

        let request = build_test_request(requested_model.clone());
        let adapter = get_adapter(channel.channel_type as i16);
        let started = Instant::now();

        let request_builder = match adapter.build_request(
            client,
            &channel.base_url,
            &api_key,
            &request,
            &actual_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                let message = format!("failed to build channel test request: {error}");
                self.mark_channel_test_failure(&channel, "build_request_error", &message)
                    .await?;
                return Ok(ChannelTestVo {
                    channel_id: channel.id,
                    success: false,
                    status_code: 0,
                    response_time: 0,
                    model: requested_model,
                    message,
                });
            }
        };

        match request_builder.send().await {
            Ok(response) => {
                let status = response.status();
                let elapsed = started.elapsed().as_millis() as i32;
                if status.is_success() {
                    let body = match response.bytes().await {
                        Ok(body) => body,
                        Err(error) => {
                            let message = format!("failed to read channel test response: {error}");
                            self.mark_channel_test_failure(
                                &channel,
                                &status.as_u16().to_string(),
                                &message,
                            )
                            .await?;
                            return Ok(ChannelTestVo {
                                channel_id: channel.id,
                                success: false,
                                status_code: status.as_u16(),
                                response_time: elapsed,
                                model: requested_model,
                                message,
                            });
                        }
                    };
                    if let Err(error) = adapter.parse_response(body, &actual_model) {
                        let message = format!("failed to parse channel test response: {error}");
                        self.mark_channel_test_failure(
                            &channel,
                            &status.as_u16().to_string(),
                            &message,
                        )
                        .await?;
                        return Ok(ChannelTestVo {
                            channel_id: channel.id,
                            success: false,
                            status_code: status.as_u16(),
                            response_time: elapsed,
                            model: requested_model,
                            message,
                        });
                    }

                    self.mark_channel_test_success(&channel, elapsed).await?;
                    Ok(ChannelTestVo {
                        channel_id: channel.id,
                        success: true,
                        status_code: status.as_u16(),
                        response_time: elapsed,
                        model: requested_model,
                        message: "ok".into(),
                    })
                } else {
                    let body = response.text().await.unwrap_or_default();
                    let message = format!("HTTP {} {}", status.as_u16(), compact_message(&body));
                    self.mark_channel_test_failure(
                        &channel,
                        &status.as_u16().to_string(),
                        &message,
                    )
                    .await?;
                    Ok(ChannelTestVo {
                        channel_id: channel.id,
                        success: false,
                        status_code: status.as_u16(),
                        response_time: elapsed,
                        model: requested_model,
                        message,
                    })
                }
            }
            Err(error) => {
                let elapsed = started.elapsed().as_millis() as i32;
                let message = format!("failed to send channel test request: {error}");
                self.mark_channel_test_failure(&channel, "request_error", &message)
                    .await?;
                Ok(ChannelTestVo {
                    channel_id: channel.id,
                    success: false,
                    status_code: 0,
                    response_time: elapsed,
                    model: requested_model,
                    message,
                })
            }
        }
    }

    async fn mark_channel_test_success(
        &self,
        channel: &channel::Model,
        elapsed: i32,
    ) -> ApiResult<()> {
        let mut active: channel::ActiveModel = channel.clone().into();
        active.response_time = Set(elapsed);
        active.failure_streak = Set(0);
        active.last_error_at = Set(None);
        active.last_error_code = Set(String::new());
        active.last_error_message = Set(String::new());
        active.last_health_status = Set(1);
        if channel.status == ChannelStatus::AutoDisabled {
            active.status = Set(ChannelStatus::Enabled);
        }
        active
            .update(&self.db)
            .await
            .context("failed to update channel test success state")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    async fn mark_channel_test_failure(
        &self,
        channel: &channel::Model,
        error_code: &str,
        message: &str,
    ) -> ApiResult<()> {
        let next_failure_streak = channel.failure_streak.saturating_add(1);
        let mut active: channel::ActiveModel = channel.clone().into();
        active.failure_streak = Set(next_failure_streak);
        active.last_error_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active.last_error_code = Set(error_code.to_string());
        active.last_error_message = Set(message.to_string());
        active.last_health_status = Set(-1);
        if channel.auto_ban && next_failure_streak >= 3 {
            active.status = Set(ChannelStatus::AutoDisabled);
        }
        active
            .update(&self.db)
            .await
            .context("failed to update channel test failure state")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    async fn find_channel<C>(db: &C, id: i64) -> ApiResult<channel::Model>
    where
        C: ConnectionTrait,
    {
        channel::Entity::find_by_id(id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(db)
            .await
            .context("failed to query channel")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("channel not found".into()))
    }

    async fn replace_channel_abilities<C>(db: &C, channel: &channel::Model) -> ApiResult<()>
    where
        C: ConnectionTrait,
    {
        ability::Entity::delete_many()
            .filter(ability::Column::ChannelId.eq(channel.id))
            .exec(db)
            .await
            .context("failed to delete channel abilities")
            .map_err(ApiErrors::Internal)?;

        let abilities = build_ability_records(channel);
        if abilities.is_empty() {
            return Ok(());
        }

        ability::Entity::insert_many(abilities)
            .exec(db)
            .await
            .context("failed to create channel abilities")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }
}

fn build_test_request(model: String) -> ChatCompletionRequest {
    ChatCompletionRequest {
        model,
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
    }
}

fn resolve_test_model(
    channel: &channel::Model,
    account: &channel_account::Model,
) -> Option<String> {
    if !channel.test_model.trim().is_empty() {
        return Some(channel.test_model.clone());
    }
    if !account.test_model.trim().is_empty() {
        return Some(account.test_model.clone());
    }
    parse_string_array(&channel.models).into_iter().next()
}

fn build_ability_records(channel: &channel::Model) -> Vec<ability::ActiveModel> {
    if channel.status != ChannelStatus::Enabled || channel.deleted_at.is_some() {
        return Vec::new();
    }

    let models = parse_string_array(&channel.models);
    if models.is_empty() {
        return Vec::new();
    }

    let scopes = {
        let scopes = parse_string_array(&channel.endpoint_scopes);
        if scopes.is_empty() {
            vec!["chat".to_string()]
        } else {
            scopes
        }
    };
    let now = chrono::Utc::now().fixed_offset();

    models
        .into_iter()
        .flat_map(|model| {
            scopes
                .iter()
                .cloned()
                .map(move |scope| ability::ActiveModel {
                    channel_group: Set(channel.channel_group.clone()),
                    endpoint_scope: Set(scope),
                    model: Set(model.clone()),
                    channel_id: Set(channel.id),
                    enabled: Set(true),
                    priority: Set(channel.priority),
                    weight: Set(channel.weight),
                    route_config: Set(serde_json::json!({})),
                    create_time: Set(now),
                    update_time: Set(now),
                    ..Default::default()
                })
        })
        .collect()
}

fn parse_string_array(value: &serde_json::Value) -> Vec<String> {
    let mut items = BTreeSet::new();
    if let Some(values) = value.as_array() {
        for item in values {
            if let Some(item) = item.as_str() {
                let trimmed = item.trim();
                if !trimmed.is_empty() {
                    items.insert(trimmed.to_string());
                }
            }
        }
    }
    items.into_iter().collect()
}

fn compact_message(message: &str) -> String {
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() <= 200 {
        compact
    } else {
        format!("{}...", compact.chars().take(200).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue::Set;

    use super::*;
    use summer_ai_model::entity::channel::{ChannelStatus, ChannelType};

    fn sample_channel(
        models: serde_json::Value,
        endpoint_scopes: serde_json::Value,
    ) -> channel::Model {
        channel::Model {
            id: 42,
            name: "primary".into(),
            channel_type: ChannelType::OpenAi,
            vendor_code: "openai".into(),
            base_url: "https://api.example.com".into(),
            status: ChannelStatus::Enabled,
            models,
            model_mapping: serde_json::json!({}),
            channel_group: "default".into(),
            endpoint_scopes,
            capabilities: serde_json::json!({}),
            weight: 10,
            priority: 100,
            config: serde_json::json!({}),
            auto_ban: true,
            test_model: String::new(),
            used_quota: 0,
            balance: 0.into(),
            balance_updated_at: None,
            response_time: 0,
            success_rate: 0.into(),
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            last_health_status: 0,
            deleted_at: None,
            remark: String::new(),
            create_by: "tester".into(),
            create_time: chrono::Utc::now().fixed_offset(),
            update_by: "tester".into(),
            update_time: chrono::Utc::now().fixed_offset(),
        }
    }

    #[test]
    fn build_ability_records_expands_models_and_scopes() {
        let channel = sample_channel(
            serde_json::json!(["gpt-4.1", "gpt-5.4"]),
            serde_json::json!(["chat", "responses"]),
        );

        let records = build_ability_records(&channel);
        assert_eq!(records.len(), 4);

        let first = &records[0];
        assert_eq!(first.channel_group, Set("default".to_string()));
        assert_eq!(first.channel_id, Set(42));
    }

    #[test]
    fn build_ability_records_defaults_to_chat_scope() {
        let channel = sample_channel(serde_json::json!(["gpt-4.1"]), serde_json::json!([]));

        let records = build_ability_records(&channel);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].endpoint_scope, Set("chat".to_string()));
    }

    #[test]
    fn parse_string_array_trims_and_deduplicates() {
        let items = parse_string_array(&serde_json::json!([" gpt-4.1 ", "", "gpt-4.1", "gpt-5"]));
        assert_eq!(items, vec!["gpt-4.1".to_string(), "gpt-5".to_string()]);
    }
}
