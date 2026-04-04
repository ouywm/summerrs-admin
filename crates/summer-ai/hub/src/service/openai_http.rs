use uuid::Uuid;

use summer_ai_core::types::chat::ChatCompletionResponse;
use summer_ai_core::types::common::{Message, Usage};
use summer_ai_core::types::responses::{
    ResponseInputTokensDetails, ResponseOutputTokensDetails, ResponseUsage, ResponsesResponse,
};
use summer_web::axum::http::{HeaderMap, HeaderValue};
use summer_web::axum::response::Response;

pub fn fallback_usage(prompt_tokens: i32) -> Usage {
    Usage {
        prompt_tokens,
        completion_tokens: 0,
        total_tokens: prompt_tokens,
        cached_tokens: 0,
        reasoning_tokens: 0,
    }
}

pub fn extract_request_id(headers: &HeaderMap) -> String {
    request_header_value(headers, &["x-request-id", "request-id"])
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

pub fn extract_upstream_request_id(headers: &HeaderMap) -> String {
    request_header_value(
        headers,
        &[
            "x-request-id",
            "request-id",
            "x-oneapi-request-id",
            "openai-request-id",
            "anthropic-request-id",
            "cf-ray",
        ],
    )
    .unwrap_or_default()
}

pub fn request_header_value(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

pub fn insert_request_id_header(response: &mut Response, request_id: &str) {
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
}

pub fn insert_upstream_request_id_header(response: &mut Response, upstream_request_id: &str) {
    if upstream_request_id.trim().is_empty() {
        return;
    }
    if let Ok(value) = HeaderValue::from_str(upstream_request_id) {
        response
            .headers_mut()
            .insert("x-upstream-request-id", value);
    }
}

pub fn response_usage_from_usage(usage: &Usage) -> ResponseUsage {
    ResponseUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: (usage.cached_tokens > 0).then_some(ResponseInputTokensDetails {
            cached_tokens: usage.cached_tokens,
        }),
        output_tokens_details: (usage.reasoning_tokens > 0).then_some(
            ResponseOutputTokensDetails {
                reasoning_tokens: usage.reasoning_tokens,
            },
        ),
    }
}

pub fn bridge_chat_completion_to_response(response: ChatCompletionResponse) -> ResponsesResponse {
    let choice = response.choices.into_iter().next();
    let output_text = choice
        .as_ref()
        .and_then(|choice| response_output_text_from_message(&choice.message));

    ResponsesResponse {
        id: response.id,
        object: "response".into(),
        created_at: response.created,
        model: response.model,
        status: "completed".into(),
        usage: Some(response_usage_from_usage(&response.usage)),
        output_text,
        extra: serde_json::Map::new(),
    }
}

fn response_output_text_from_message(message: &Message) -> Option<String> {
    match &message.content {
        serde_json::Value::String(text) if !text.is_empty() => Some(text.clone()),
        serde_json::Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| item.get("text").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::types::chat::{ChatCompletionResponse, Choice};
    use summer_ai_core::types::common::{FinishReason, Message};
    use summer_web::axum::body::Body;
    use summer_web::axum::http::Response as AxumResponse;

    #[test]
    fn request_id_helpers_roundtrip_headers() {
        let mut response = AxumResponse::new(Body::empty());
        insert_request_id_header(&mut response, "req_123");
        insert_upstream_request_id_header(&mut response, "up_456");

        assert_eq!(response.headers()["x-request-id"], "req_123");
        assert_eq!(response.headers()["x-upstream-request-id"], "up_456");
    }

    #[test]
    fn bridge_chat_completion_to_response_preserves_usage_and_text() {
        let response = ChatCompletionResponse {
            id: "chatcmpl_123".into(),
            object: "chat.completion".into(),
            created: 1,
            model: "gpt-5".into(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".into(),
                    content: serde_json::json!("hello"),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                finish_reason: Some(FinishReason::Stop),
            }],
            usage: Usage {
                prompt_tokens: 3,
                completion_tokens: 4,
                total_tokens: 7,
                cached_tokens: 1,
                reasoning_tokens: 2,
            },
        };

        let bridged = bridge_chat_completion_to_response(response);
        assert_eq!(bridged.id, "chatcmpl_123");
        assert_eq!(bridged.output_text.as_deref(), Some("hello"));
        assert_eq!(
            bridged.usage.as_ref().map(|usage| usage.total_tokens),
            Some(7)
        );
        assert_eq!(
            bridged
                .usage
                .as_ref()
                .and_then(|usage| usage.input_tokens_details.as_ref())
                .map(|details| details.cached_tokens),
            Some(1)
        );
    }
}
