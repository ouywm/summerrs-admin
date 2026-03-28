use std::collections::BTreeMap;

use futures::StreamExt;
use summer_web::axum::response::sse::{Event, KeepAlive, Sse};
use summer_web::axum::response::{IntoResponse, Response};

use summer_ai_core::types::chat::ChatCompletionChunk;
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::completion::CompletionChunk;
use summer_ai_core::types::responses::{
    ResponsesOutputContent, ResponsesOutputItem, ResponsesRequest, ResponsesResponse,
    ResponsesUsage,
};

use crate::relay::billing::{BillingEngine, ModelConfigInfo};
use crate::relay::channel_router::SelectedChannel;
use crate::service::log::{ChatCompletionLogRecord, LogService};
use crate::service::token::TokenInfo;

/// Convert the upstream chunk stream into an axum SSE response.
#[allow(clippy::too_many_arguments)]
pub fn build_sse_response(
    chunk_stream: futures::stream::BoxStream<'static, anyhow::Result<ChatCompletionChunk>>,
    token_info: TokenInfo,
    pre_consumed: i64,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    channel: SelectedChannel,
    requested_model: String,
    start_elapsed: i64,
    client_ip: String,
    log_svc: LogService,
    billing: BillingEngine,
) -> Response {
    let stream = async_stream::stream! {
        let mut last_usage: Option<Usage> = None;
        let mut first_token_time: Option<i64> = None;
        let start = std::time::Instant::now();
        let mut upstream_model = String::new();

        tokio::pin!(chunk_stream);
        while let Some(result) = chunk_stream.next().await {
            match result {
                Ok(chunk) => {
                    if first_token_time.is_none() && !chunk.choices.is_empty() {
                        first_token_time = Some(start.elapsed().as_millis() as i64);
                    }
                    if chunk.usage.is_some() {
                        last_usage = chunk.usage.clone();
                    }
                    if upstream_model.is_empty() {
                        upstream_model = chunk.model.clone();
                    }
                    match serde_json::to_string(&chunk) {
                        Ok(json) => yield Ok::<_, std::convert::Infallible>(Event::default().data(json)),
                        Err(e) => {
                            tracing::error!("failed to serialize chat chunk: {e}");
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Stream error: {e}");
                    break;
                }
            }
        }

        yield Ok(Event::default().data("[DONE]"));

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        let ftt = first_token_time.unwrap_or(0) as i32;

        if let Some(usage) = last_usage {
            let usage_clone = usage.clone();
            tokio::spawn(async move {
                let logged_quota =
                    BillingEngine::calculate_actual_quota(&usage_clone, &model_config, group_ratio);
                let actual_quota = match billing
                    .post_consume(
                        &token_info,
                        pre_consumed,
                        &usage_clone,
                        &model_config,
                        group_ratio,
                    )
                    .await
                {
                    Ok(quota) => quota,
                    Err(error) => {
                        tracing::error!("failed to settle stream usage asynchronously: {error}");
                        logged_quota
                    }
                };

                log_svc.record_chat_completion_async(
                    &token_info,
                    &channel,
                    &usage,
                    ChatCompletionLogRecord {
                        endpoint: "chat/completions".into(),
                        requested_model,
                        upstream_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: total_elapsed as i32,
                        first_token_time: ftt,
                        is_stream: true,
                        client_ip,
                    },
                );
            });
        } else {
            billing.refund_later(token_info.token_id, pre_consumed);
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[allow(clippy::too_many_arguments)]
pub fn build_completion_sse_response(
    chunk_stream: futures::stream::BoxStream<'static, anyhow::Result<CompletionChunk>>,
    token_info: TokenInfo,
    pre_consumed: i64,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    channel: SelectedChannel,
    requested_model: String,
    start_elapsed: i64,
    client_ip: String,
    log_svc: LogService,
    billing: BillingEngine,
) -> Response {
    let stream = async_stream::stream! {
        let mut last_usage: Option<Usage> = None;
        let mut first_token_time: Option<i64> = None;
        let start = std::time::Instant::now();
        let mut upstream_model = String::new();

        tokio::pin!(chunk_stream);
        while let Some(result) = chunk_stream.next().await {
            match result {
                Ok(chunk) => {
                    if first_token_time.is_none() && !chunk.choices.is_empty() {
                        first_token_time = Some(start.elapsed().as_millis() as i64);
                    }
                    if chunk.usage.is_some() {
                        last_usage = chunk.usage.clone();
                    }
                    if upstream_model.is_empty() {
                        upstream_model = chunk.model.clone();
                    }
                    match serde_json::to_string(&chunk) {
                        Ok(json) => yield Ok::<_, std::convert::Infallible>(Event::default().data(json)),
                        Err(error) => {
                            tracing::error!("failed to serialize completion chunk: {error}");
                            break;
                        }
                    }
                }
                Err(error) => {
                    tracing::error!("completion stream error: {error}");
                    break;
                }
            }
        }

        yield Ok(Event::default().data("[DONE]"));

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        let ftt = first_token_time.unwrap_or(0) as i32;

        if let Some(usage) = last_usage {
            let usage_clone = usage.clone();
            tokio::spawn(async move {
                let logged_quota =
                    BillingEngine::calculate_actual_quota(&usage_clone, &model_config, group_ratio);
                let actual_quota = match billing
                    .post_consume(
                        &token_info,
                        pre_consumed,
                        &usage_clone,
                        &model_config,
                        group_ratio,
                    )
                    .await
                {
                    Ok(quota) => quota,
                    Err(error) => {
                        tracing::error!("failed to settle completion stream usage asynchronously: {error}");
                        logged_quota
                    }
                };

                log_svc.record_endpoint_usage_async(
                    &token_info,
                    &channel,
                    &usage,
                    crate::service::log::EndpointUsageLogRecord {
                        endpoint: "completions".into(),
                        requested_model,
                        upstream_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: total_elapsed as i32,
                        first_token_time: ftt,
                        is_stream: true,
                        client_ip,
                    },
                );
            });
        } else {
            billing.refund_later(token_info.token_id, pre_consumed);
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Convert the upstream chat chunk stream into a minimal Responses API SSE stream.
#[allow(clippy::too_many_arguments)]
pub fn build_responses_sse_response(
    chunk_stream: futures::stream::BoxStream<'static, anyhow::Result<ChatCompletionChunk>>,
    request: ResponsesRequest,
    token_info: TokenInfo,
    pre_consumed: i64,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    channel: SelectedChannel,
    requested_model: String,
    start_elapsed: i64,
    client_ip: String,
    log_svc: LogService,
    billing: BillingEngine,
) -> Response {
    let stream = async_stream::stream! {
        let mut state = ResponsesStreamState::new(&request);
        let start = std::time::Instant::now();
        let mut last_usage: Option<Usage> = None;
        let mut first_token_time: Option<i64> = None;
        let mut upstream_model = String::new();

        tokio::pin!(chunk_stream);
        while let Some(result) = chunk_stream.next().await {
            match result {
                Ok(chunk) => {
                    if first_token_time.is_none() && !chunk.choices.is_empty() {
                        first_token_time = Some(start.elapsed().as_millis() as i64);
                    }
                    if chunk.usage.is_some() {
                        last_usage = chunk.usage.clone();
                    }
                    if upstream_model.is_empty() {
                        upstream_model = chunk.model.clone();
                    }

                    if state.response_id.is_none() {
                        state.initialize(&chunk);
                        if let Some(created) = state.created_event() {
                            yield Ok::<_, std::convert::Infallible>(created);
                        }
                        if let Some(in_progress) = state.in_progress_event() {
                            yield Ok(in_progress);
                        }
                    }

                    for choice in &chunk.choices {
                        if let Some(text_delta) = choice.delta.content.as_deref()
                            && !text_delta.is_empty()
                        {
                            if let Some(event) = state.ensure_text_item_added() {
                                yield Ok(event);
                            }
                            state.text_delta.push_str(text_delta);
                            yield Ok(responses_event(
                                "response.output_text.delta",
                                serde_json::json!({
                                    "type": "response.output_text.delta",
                                    "output_index": state.text_output_index,
                                    "item_id": state.text_item_id,
                                    "content_index": 0,
                                    "delta": text_delta,
                                }),
                            ));
                        }

                        if let Some(tool_calls) = choice.delta.tool_calls.as_ref() {
                            for tool_call in tool_calls {
                                if !state.tool_calls.contains_key(&tool_call.index) {
                                    let output_index = state.next_output_index();
                                    let response_id =
                                        state.response_id.as_deref().unwrap_or("resp").to_string();
                                    state.tool_calls.insert(
                                        tool_call.index,
                                        ResponsesStreamToolCall::new(
                                            output_index,
                                            &response_id,
                                            tool_call.index,
                                        ),
                                    );
                                }
                                let entry = state
                                    .tool_calls
                                    .get_mut(&tool_call.index)
                                    .expect("tool call state should exist");

                                if let Some(call_id) = tool_call.id.as_ref()
                                    && !call_id.is_empty()
                                {
                                    entry.call_id = call_id.clone();
                                }
                                if let Some(function) = tool_call.function.as_ref() {
                                    if let Some(name) = function.name.as_ref()
                                        && !name.is_empty()
                                    {
                                        entry.name = name.clone();
                                    }
                                    if let Some(arguments_delta) = function.arguments.as_ref()
                                        && !arguments_delta.is_empty()
                                    {
                                        entry.arguments.push_str(arguments_delta);
                                        if !entry.added {
                                            entry.added = true;
                                            yield Ok(responses_event(
                                                "response.output_item.added",
                                                serde_json::json!({
                                                    "type": "response.output_item.added",
                                                    "output_index": entry.output_index,
                                                    "item": {
                                                        "id": entry.item_id,
                                                        "type": "function_call",
                                                        "status": "in_progress",
                                                        "call_id": entry.call_id,
                                                        "name": entry.name,
                                                        "arguments": entry.arguments,
                                                    }
                                                }),
                                            ));
                                        }

                                        yield Ok(responses_event(
                                            "response.function_call_arguments.delta",
                                            serde_json::json!({
                                                "type": "response.function_call_arguments.delta",
                                                "output_index": entry.output_index,
                                                "item_id": entry.item_id,
                                                "delta": arguments_delta,
                                            }),
                                        ));
                                    }
                                }
                            }
                        }

                        if let Some(finish_reason) = choice.finish_reason.as_ref() {
                            state.finish_reason = Some(format!("{finish_reason:?}"));
                            if state.text_item_added && !state.text_delta.is_empty() {
                                let output_item = state.take_text_output_item();
                                yield Ok(responses_event(
                                    "response.output_text.done",
                                    serde_json::json!({
                                        "type": "response.output_text.done",
                                        "output_index": output_item.0,
                                        "item_id": output_item.1.id,
                                        "content_index": 0,
                                        "text": output_item.2,
                                    }),
                                ));
                                yield Ok(responses_event(
                                    "response.output_item.done",
                                    serde_json::json!({
                                        "type": "response.output_item.done",
                                        "output_index": output_item.0,
                                        "item": output_item.1,
                                    }),
                                ));
                            }

                            if matches!(finish_reason, summer_ai_core::types::common::FinishReason::ToolCalls) {
                                for tool_call in state.take_tool_outputs() {
                                    yield Ok(responses_event(
                                        "response.function_call_arguments.done",
                                        serde_json::json!({
                                            "type": "response.function_call_arguments.done",
                                            "output_index": tool_call.0,
                                            "item_id": tool_call.1.id,
                                            "arguments": tool_call.2,
                                        }),
                                    ));
                                    yield Ok(responses_event(
                                        "response.output_item.done",
                                        serde_json::json!({
                                            "type": "response.output_item.done",
                                            "output_index": tool_call.0,
                                            "item": tool_call.1,
                                        }),
                                    ));
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    tracing::error!("responses stream error: {error}");
                    break;
                }
            }
        }

        if state.text_item_added && !state.text_delta.is_empty() {
            let output_item = state.take_text_output_item();
            yield Ok(responses_event(
                "response.output_text.done",
                serde_json::json!({
                    "type": "response.output_text.done",
                    "output_index": output_item.0,
                    "item_id": output_item.1.id,
                    "content_index": 0,
                    "text": output_item.2,
                }),
            ));
            yield Ok(responses_event(
                "response.output_item.done",
                serde_json::json!({
                    "type": "response.output_item.done",
                    "output_index": output_item.0,
                    "item": output_item.1,
                }),
            ));
        }

        for tool_call in state.take_tool_outputs() {
            yield Ok(responses_event(
                "response.function_call_arguments.done",
                serde_json::json!({
                    "type": "response.function_call_arguments.done",
                    "output_index": tool_call.0,
                    "item_id": tool_call.1.id,
                    "arguments": tool_call.2,
                }),
            ));
            yield Ok(responses_event(
                "response.output_item.done",
                serde_json::json!({
                    "type": "response.output_item.done",
                    "output_index": tool_call.0,
                    "item": tool_call.1,
                }),
            ));
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        let ftt = first_token_time.unwrap_or(0) as i32;
        let usage = last_usage.clone().unwrap_or_default();
        let completed_response = state.completed_response(
            upstream_model.clone(),
            ResponsesUsage::from_usage(&usage),
        );
        yield Ok(responses_event(
            "response.completed",
            serde_json::json!({
                "type": "response.completed",
                "response": completed_response,
            }),
        ));

        if let Some(usage) = last_usage {
            let usage_clone = usage.clone();
            tokio::spawn(async move {
                let logged_quota =
                    BillingEngine::calculate_actual_quota(&usage_clone, &model_config, group_ratio);
                let actual_quota = match billing
                    .post_consume(
                        &token_info,
                        pre_consumed,
                        &usage_clone,
                        &model_config,
                        group_ratio,
                    )
                    .await
                {
                    Ok(quota) => quota,
                    Err(error) => {
                        tracing::error!("failed to settle responses stream usage asynchronously: {error}");
                        logged_quota
                    }
                };

                log_svc.record_chat_completion_async(
                    &token_info,
                    &channel,
                    &usage,
                    ChatCompletionLogRecord {
                        endpoint: "responses".into(),
                        requested_model,
                        upstream_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: total_elapsed as i32,
                        first_token_time: ftt,
                        is_stream: true,
                        client_ip,
                    },
                );
            });
        } else {
            billing.refund_later(token_info.token_id, pre_consumed);
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn responses_event(event_name: &str, payload: serde_json::Value) -> Event {
    Event::default().event(event_name).data(payload.to_string())
}

struct ResponsesStreamState {
    response_id: Option<String>,
    created_at: i64,
    request: ResponsesRequest,
    model: String,
    output: Vec<ResponsesOutputItem>,
    output_text: String,
    text_item_id: String,
    text_item_added: bool,
    text_output_index: usize,
    text_delta: String,
    next_output_index: usize,
    tool_calls: BTreeMap<i32, ResponsesStreamToolCall>,
    finish_reason: Option<String>,
}

impl ResponsesStreamState {
    fn new(request: &ResponsesRequest) -> Self {
        Self {
            response_id: None,
            created_at: chrono::Utc::now().timestamp(),
            request: request.clone(),
            model: request.model.clone(),
            output: Vec::new(),
            output_text: String::new(),
            text_item_id: String::new(),
            text_item_added: false,
            text_output_index: 0,
            text_delta: String::new(),
            next_output_index: 0,
            tool_calls: BTreeMap::new(),
            finish_reason: None,
        }
    }

    fn initialize(&mut self, chunk: &ChatCompletionChunk) {
        self.response_id = Some(chunk.id.clone());
        self.created_at = chunk.created;
        self.model = chunk.model.clone();
        self.text_item_id = format!("msg_{}", chunk.id);
        self.text_output_index = self.next_output_index();
    }

    fn created_event(&self) -> Option<Event> {
        Some(responses_event(
            "response.created",
            serde_json::json!({
                "type": "response.created",
                "response": self.base_response("in_progress", ResponsesUsage::from_usage(&Usage::default())),
            }),
        ))
    }

    fn in_progress_event(&self) -> Option<Event> {
        Some(responses_event(
            "response.in_progress",
            serde_json::json!({
                "type": "response.in_progress",
                "response": self.base_response("in_progress", ResponsesUsage::from_usage(&Usage::default())),
            }),
        ))
    }

    fn ensure_text_item_added(&mut self) -> Option<Event> {
        if self.text_item_added {
            return None;
        }

        self.text_item_added = true;
        Some(responses_event(
            "response.output_item.added",
            serde_json::json!({
                "type": "response.output_item.added",
                "output_index": self.text_output_index,
                "item": {
                    "id": self.text_item_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": []
                }
            }),
        ))
    }

    fn take_text_output_item(&mut self) -> (usize, ResponsesOutputItem, String) {
        let text = std::mem::take(&mut self.text_delta);
        self.output_text.push_str(&text);
        self.text_item_added = false;

        let item = ResponsesOutputItem {
            id: self.text_item_id.clone(),
            r#type: "message".into(),
            status: response_status(self.finish_reason.as_deref()).into(),
            role: Some("assistant".into()),
            content: Some(vec![ResponsesOutputContent {
                r#type: "output_text".into(),
                text: text.clone(),
            }]),
            call_id: None,
            name: None,
            arguments: None,
        };
        self.output.push(item.clone());
        (self.text_output_index, item, text)
    }

    fn take_tool_outputs(&mut self) -> Vec<(usize, ResponsesOutputItem, String)> {
        let states = std::mem::take(&mut self.tool_calls);
        states
            .into_values()
            .map(|tool_call| {
                let item = ResponsesOutputItem {
                    id: tool_call.item_id.clone(),
                    r#type: "function_call".into(),
                    status: "completed".into(),
                    role: None,
                    content: None,
                    call_id: Some(tool_call.call_id.clone()),
                    name: Some(tool_call.name.clone()),
                    arguments: Some(tool_call.arguments.clone()),
                };
                self.output.push(item.clone());
                (tool_call.output_index, item, tool_call.arguments)
            })
            .collect()
    }

    fn completed_response(
        &self,
        upstream_model: String,
        usage: ResponsesUsage,
    ) -> ResponsesResponse {
        let mut response =
            self.base_response(response_status(self.finish_reason.as_deref()), usage);
        response.model = if upstream_model.is_empty() {
            self.model.clone()
        } else {
            upstream_model
        };
        response.output = self.output.clone();
        response.output_text = if self.output_text.is_empty() {
            None
        } else {
            Some(self.output_text.clone())
        };
        response
    }

    fn base_response(&self, status: &str, usage: ResponsesUsage) -> ResponsesResponse {
        ResponsesResponse {
            id: self
                .response_id
                .clone()
                .unwrap_or_else(|| "resp_pending".into()),
            object: "response".into(),
            created_at: self.created_at,
            model: self.model.clone(),
            status: status.into(),
            output: Vec::new(),
            output_text: None,
            usage,
            incomplete_details: match status {
                "incomplete" => Some(
                    summer_ai_core::types::responses::ResponsesIncompleteDetails {
                        reason: "max_output_tokens".into(),
                    },
                ),
                "failed" => Some(
                    summer_ai_core::types::responses::ResponsesIncompleteDetails {
                        reason: "content_filter".into(),
                    },
                ),
                _ => None,
            },
            text: Some(
                summer_ai_core::types::responses::ResponsesOutputTextConfig {
                    format: summer_ai_core::types::responses::ResponsesOutputTextFormat {
                        r#type: self
                            .request
                            .text
                            .as_ref()
                            .and_then(|text| text.format.as_ref())
                            .and_then(|format| format.get("type"))
                            .and_then(|format| format.as_str())
                            .unwrap_or("text")
                            .to_string(),
                    },
                },
            ),
        }
    }

    fn next_output_index(&mut self) -> usize {
        let index = self.next_output_index;
        self.next_output_index += 1;
        index
    }
}

struct ResponsesStreamToolCall {
    item_id: String,
    output_index: usize,
    call_id: String,
    name: String,
    arguments: String,
    added: bool,
}

impl ResponsesStreamToolCall {
    fn new(output_index: usize, response_id: &str, tool_index: i32) -> Self {
        Self {
            item_id: format!("fc_{response_id}_{tool_index}"),
            output_index,
            call_id: format!("call_{response_id}_{tool_index}"),
            name: String::new(),
            arguments: String::new(),
            added: false,
        }
    }
}

fn response_status(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("Length") => "incomplete",
        Some("ContentFilter") => "failed",
        _ => "completed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> ResponsesRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4",
            "input": "hello"
        }))
        .unwrap()
    }

    #[test]
    fn response_status_maps_finish_reasons() {
        assert_eq!(response_status(Some("Length")), "incomplete");
        assert_eq!(response_status(Some("ContentFilter")), "failed");
        assert_eq!(response_status(Some("ToolCalls")), "completed");
        assert_eq!(response_status(None), "completed");
    }

    #[test]
    fn responses_stream_state_builds_completed_response_with_text_output() {
        let mut state = ResponsesStreamState::new(&sample_request());
        state.response_id = Some("resp_123".into());
        state.created_at = 1_700_000_000;
        state.text_item_id = "msg_resp_123".into();
        state.text_output_index = 0;
        state.text_item_added = true;
        state.text_delta = "hello world".into();
        state.finish_reason = Some("ToolCalls".into());

        let _ = state.take_text_output_item();
        let response = state.completed_response(
            "gpt-5.4".into(),
            ResponsesUsage::from_usage(&Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cached_tokens: 0,
                reasoning_tokens: 0,
            }),
        );

        assert_eq!(response.id, "resp_123");
        assert_eq!(response.output_text.as_deref(), Some("hello world"));
        assert_eq!(response.output.len(), 1);
        assert_eq!(response.output[0].r#type, "message");
    }
}
