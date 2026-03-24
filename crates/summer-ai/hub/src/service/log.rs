use sea_orm::prelude::BigDecimal;
use summer::plugin::Service;

use summer_ai_core::types::common::Usage;
use summer_ai_model::dto::log::CreateLogDto;
use summer_ai_model::entity::log::{LogStatus, LogType};

use crate::relay::channel_router::SelectedChannel;
use crate::service::log_batch::AiLogBatchQueue;
use crate::service::token::TokenInfo;

pub struct ChatCompletionLogRecord {
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub quota: i64,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub is_stream: bool,
    pub client_ip: String,
}

#[derive(Clone, Service)]
pub struct LogService {
    #[inject(component)]
    queue: AiLogBatchQueue,
}

impl LogService {
    /// Queue one AI usage log for batched persistence.
    pub fn record_async(&self, dto: CreateLogDto) {
        self.queue.push(dto.into());
    }

    pub fn record_chat_completion_async(
        &self,
        token_info: &TokenInfo,
        channel: &SelectedChannel,
        usage: &Usage,
        record: ChatCompletionLogRecord,
    ) {
        self.record_async(CreateLogDto {
            user_id: token_info.user_id,
            token_id: token_info.token_id,
            token_name: token_info.name.clone(),
            project_id: 0,
            conversation_id: 0,
            message_id: 0,
            session_id: 0,
            thread_id: 0,
            trace_id: 0,
            channel_id: channel.channel_id,
            channel_name: channel.channel_name.clone(),
            account_id: channel.account_id,
            account_name: channel.account_name.clone(),
            execution_id: 0,
            endpoint: "chat/completions".into(),
            request_format: "openai".into(),
            requested_model: record.requested_model,
            upstream_model: record.upstream_model,
            model_name: record.model_name,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_tokens: usage.cached_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            quota: record.quota,
            cost_total: BigDecimal::from(0),
            price_reference: String::new(),
            elapsed_time: record.elapsed_time,
            first_token_time: record.first_token_time,
            is_stream: record.is_stream,
            request_id: String::new(),
            upstream_request_id: String::new(),
            status_code: 200,
            client_ip: record.client_ip,
            user_agent: String::new(),
            content: String::new(),
            log_type: LogType::Consume,
            status: LogStatus::Success,
        });
    }
}
