use super::*;
use crate::provider::{ChatProvider, Provider, ProviderErrorKind};
use crate::types::common::FinishReason;
use futures::{StreamExt, stream};
use http::StatusCode;

fn sample_request() -> ChatCompletionRequest {
    serde_json::from_value(serde_json::json!({
        "model": "claude-3-5-sonnet",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather info",
                "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
            }
        }]
    }))
    .unwrap()
}

#[test]
fn build_request_targets_messages_endpoint() {
    let client = reqwest::Client::new();
    let adapter = AnthropicAdapter;
    let builder = adapter
        .build_chat_request(
            &client,
            "https://api.anthropic.com",
            "sk-ant-test",
            &sample_request(),
            "claude-3-5-sonnet-20241022",
        )
        .unwrap();

    let request = builder.build().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://api.anthropic.com/v1/messages"
    );
    assert_eq!(request.headers().get("x-api-key").unwrap(), "sk-ant-test");
    assert_eq!(
        request.headers().get("anthropic-version").unwrap(),
        "2023-06-01"
    );
}

#[test]
fn build_request_omits_tool_choice_and_tools_for_none_and_converts_data_url_image() {
    let client = reqwest::Client::new();
    let adapter = AnthropicAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "claude-3-5-sonnet",
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "parameters": {"type": "object"}
            }
        }],
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "What is in this image?"},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "data:image/png;base64,aGVsbG8="
                    }
                }
            ]
        }],
        "tool_choice": "none"
    }))
    .unwrap();

    let request = adapter
        .build_chat_request(
            &client,
            "https://api.anthropic.com",
            "sk-ant-test",
            &req,
            "claude-3-5-sonnet-20241022",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert!(body.get("tool_choice").is_none());
    assert!(body.get("tools").is_none());
    assert_eq!(
        body["messages"][0]["content"][1],
        serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/png",
                "data": "aGVsbG8="
            }
        })
    );
}

#[test]
fn build_request_preserves_thinking_extra_body_fields() {
    let client = reqwest::Client::new();
    let adapter = AnthropicAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "claude-sonnet-4-20250514",
        "messages": [{"role": "user", "content": "solve a hard problem"}],
        "max_tokens": 4096,
        "thinking": {
            "type": "enabled",
            "budget_tokens": 2048
        }
    }))
    .unwrap();

    let request = adapter
        .build_chat_request(
            &client,
            "https://api.anthropic.com",
            "sk-ant-test",
            &req,
            "claude-sonnet-4-20250514",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["thinking"],
        serde_json::json!({
            "type": "enabled",
            "budget_tokens": 2048
        })
    );
}

#[test]
fn build_request_promotes_developer_messages_into_system_prompt() {
    let client = reqwest::Client::new();
    let adapter = AnthropicAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "claude-3-5-sonnet",
        "messages": [
            {"role": "system", "content": "Follow platform rules."},
            {"role": "developer", "content": "Always answer with JSON."},
            {"role": "user", "content": "hello"}
        ]
    }))
    .unwrap();

    let request = adapter
        .build_chat_request(
            &client,
            "https://api.anthropic.com",
            "sk-ant-test",
            &req,
            "claude-3-5-sonnet-20241022",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["system"],
        serde_json::json!("Follow platform rules.\n\nAlways answer with JSON.")
    );
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"][0]["text"], "hello");
}

#[test]
fn build_request_preserves_tool_result_content_blocks() {
    let client = reqwest::Client::new();
    let adapter = AnthropicAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "claude-3-5-sonnet",
        "messages": [
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "toolu_123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"city\":\"Paris\"}"
                    }
                }]
            },
            {
                "role": "tool",
                "tool_call_id": "toolu_123",
                "content": [{
                    "type": "text",
                    "text": "15C and sunny"
                }]
            }
        ]
    }))
    .unwrap();

    let request = adapter
        .build_chat_request(
            &client,
            "https://api.anthropic.com",
            "sk-ant-test",
            &req,
            "claude-3-5-sonnet-20241022",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["messages"][1]["content"][0],
        serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_123",
            "content": [{
                "type": "text",
                "text": "15C and sunny"
            }]
        })
    );
}

#[test]
fn build_request_converts_system_tool_result_and_named_tool_choice() {
    let client = reqwest::Client::new();
    let adapter = AnthropicAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "claude-3-5-sonnet",
        "messages": [
            {"role": "system", "content": "Be concise."},
            {"role": "user", "content": "What's the weather in Paris?"},
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "toolu_123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"city\":\"Paris\"}"
                    }
                }]
            },
            {
                "role": "tool",
                "tool_call_id": "toolu_123",
                "content": "15C and sunny"
            }
        ],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather",
                "parameters": null
            }
        }],
        "tool_choice": {
            "type": "function",
            "function": {"name": "get_weather"}
        },
        "stop": ["END", "HALT"]
    }))
    .unwrap();

    let request = adapter
        .build_chat_request(
            &client,
            "https://api.anthropic.com",
            "sk-ant-test",
            &req,
            "claude-3-5-sonnet-20241022",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["system"], serde_json::json!("Be concise."));
    assert_eq!(
        body["tool_choice"],
        serde_json::json!({"type": "tool", "name": "get_weather"})
    );
    assert_eq!(body["stop_sequences"], serde_json::json!(["END", "HALT"]));
    assert_eq!(
        body["tools"][0],
        serde_json::json!({
            "name": "get_weather",
            "description": "Get weather",
            "input_schema": {"type": "object"}
        })
    );
    assert_eq!(
        body["messages"][1]["content"][0],
        serde_json::json!({
            "type": "tool_use",
            "id": "toolu_123",
            "name": "get_weather",
            "input": {"city": "Paris"}
        })
    );
    assert_eq!(
        body["messages"][2]["content"][0],
        serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_123",
            "content": "15C and sunny"
        })
    );
}

#[test]
fn parse_response_converts_text_and_usage() {
    let adapter = AnthropicAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "id": "msg_123",
            "model": "claude-3-5-sonnet-20241022",
            "content": [{"type": "text", "text": "Hello from Claude"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        }))
        .unwrap(),
    );

    let response = adapter.parse_chat_response(body, "claude").unwrap();
    assert_eq!(response.id, "msg_123");
    assert_eq!(response.model, "claude-3-5-sonnet-20241022");
    assert_eq!(
        response.choices[0].message.content,
        serde_json::Value::String("Hello from Claude".into())
    );
    assert_eq!(response.usage.total_tokens, 19);
}

#[test]
fn parse_response_converts_tool_use_and_cached_usage() {
    let adapter = AnthropicAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "id": "msg_456",
            "model": "claude-3-5-sonnet-20241022",
            "content": [{
                "type": "tool_use",
                "id": "toolu_123",
                "name": "get_weather",
                "input": {"city": "Paris"}
            }],
            "stop_reason": "tool_use",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7,
                "cache_read_input_tokens": 5,
                "cache_creation_input_tokens": 3
            }
        }))
        .unwrap(),
    );

    let response = adapter.parse_chat_response(body, "claude").unwrap();
    let tool_calls = response.choices[0].message.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "toolu_123");
    assert_eq!(tool_calls[0].function.name, "get_weather");
    assert_eq!(tool_calls[0].function.arguments, "{\"city\":\"Paris\"}");
    assert!(matches!(
        response.choices[0].finish_reason,
        Some(FinishReason::ToolCalls)
    ));
    assert_eq!(response.usage.prompt_tokens, 12);
    assert_eq!(response.usage.completion_tokens, 7);
    assert_eq!(response.usage.total_tokens, 19);
    assert_eq!(response.usage.cached_tokens, 8);
}

#[test]
fn parse_response_maps_refusal_finish_reason_to_content_filter() {
    let adapter = AnthropicAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "id": "msg_refusal",
            "model": "claude-sonnet-4-20250514",
            "content": [{"type": "text", "text": "I can’t help with that."}],
            "stop_reason": "refusal",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        }))
        .unwrap(),
    );

    let response = adapter.parse_chat_response(body, "claude").unwrap();
    assert!(matches!(
        response.choices[0].finish_reason,
        Some(FinishReason::ContentFilter)
    ));
}

#[test]
fn parse_response_maps_max_tokens_finish_reason_to_length() {
    let adapter = AnthropicAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "id": "msg_length",
            "model": "claude-sonnet-4-20250514",
            "content": [{"type": "text", "text": "Partial answer"}],
            "stop_reason": "max_tokens",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        }))
        .unwrap(),
    );

    let response = adapter.parse_chat_response(body, "claude").unwrap();
    assert!(matches!(
        response.choices[0].finish_reason,
        Some(FinishReason::Length)
    ));
}

#[tokio::test]
async fn parse_stream_emits_text_and_final_usage() {
    let adapter = AnthropicAdapter;
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3-5-sonnet-20241022\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0,\"cache_read_input_tokens\":5,\"cache_creation_input_tokens\":3}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":7},\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "claude-3-5-sonnet-20241022")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert!(
        chunks
            .iter()
            .any(|chunk| { chunk.choices[0].delta.role.as_deref() == Some("assistant") })
    );
    assert!(
        chunks
            .iter()
            .any(|chunk| { chunk.choices[0].delta.content.as_deref() == Some("Hello") })
    );
    let final_chunk = chunks
        .iter()
        .find(|chunk| chunk.usage.is_some())
        .expect("expected usage chunk");
    assert_eq!(
        final_chunk.usage.as_ref().map(|usage| usage.total_tokens),
        Some(19)
    );
    assert_eq!(
        final_chunk.usage.as_ref().map(|usage| usage.prompt_tokens),
        Some(12)
    );
    assert_eq!(
        final_chunk.usage.as_ref().map(|usage| usage.cached_tokens),
        Some(8)
    );
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::Stop)
    ));
}

#[tokio::test]
async fn parse_stream_preserves_utf8_when_sse_chunk_splits_multibyte_boundary() {
    let adapter = AnthropicAdapter;
    let event = concat!(
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,",
        "\"delta\":{\"type\":\"text_delta\",\"text\":\"你好\"}}\n\n"
    );
    let bytes = event.as_bytes();
    let split_at = bytes
        .windows("你".len())
        .position(|window| window == "你".as_bytes())
        .expect("utf8 boundary")
        + 1;
    let chunks = vec![
        Ok::<_, std::io::Error>(Bytes::copy_from_slice(&bytes[..split_at])),
        Ok::<_, std::io::Error>(Bytes::copy_from_slice(&bytes[split_at..])),
    ];
    let mock_response = http::Response::builder()
        .status(200)
        .body(reqwest::Body::wrap_stream(stream::iter(chunks)))
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "claude-sonnet-4-20250514")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert!(
        chunks
            .iter()
            .any(|chunk| chunk.choices[0].delta.content.as_deref() == Some("你好"))
    );
}

#[tokio::test]
async fn parse_stream_emits_reasoning_content_for_thinking_delta() {
    let adapter = AnthropicAdapter;
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"message\":{\"id\":\"msg_think\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me think this through.\"}}\n\n",
        "event: message_delta\n",
        "data: {\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "claude-sonnet-4-20250514")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert!(
        chunks
            .iter()
            .any(|chunk| chunk.choices[0].delta.role.as_deref() == Some("assistant"))
    );
    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].delta.reasoning_content.as_deref() == Some("Let me think this through.")
    }));
    let final_chunk = chunks
        .iter()
        .find(|chunk| chunk.usage.is_some())
        .expect("expected usage chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::Stop)
    ));
    assert_eq!(
        final_chunk.usage.as_ref().map(|usage| usage.total_tokens),
        Some(14)
    );
}

#[tokio::test]
async fn parse_stream_maps_content_filter_finish_reason() {
    let adapter = AnthropicAdapter;
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"message\":{\"id\":\"msg_filter\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
        "event: message_delta\n",
        "data: {\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"delta\":{\"stop_reason\":\"content_filter\"}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "claude-sonnet-4-20250514")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    let final_chunk = chunks
        .iter()
        .find(|chunk| chunk.usage.is_some())
        .expect("expected usage chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::ContentFilter)
    ));
}

#[tokio::test]
async fn parse_stream_uses_event_name_when_type_is_missing() {
    let adapter = AnthropicAdapter;
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"message\":{\"id\":\"msg_event_name\",\"model\":\"claude-3-5-sonnet-20241022\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello from fallback\"}}\n\n",
        "event: message_delta\n",
        "data: {\"usage\":{\"input_tokens\":12,\"output_tokens\":7},\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "claude-3-5-sonnet-20241022")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert!(
        chunks
            .iter()
            .any(|chunk| chunk.choices[0].delta.role.as_deref() == Some("assistant"))
    );
    assert!(
        chunks.iter().any(|chunk| {
            chunk.choices[0].delta.content.as_deref() == Some("Hello from fallback")
        })
    );
    let final_chunk = chunks
        .iter()
        .find(|chunk| chunk.usage.is_some())
        .expect("expected usage chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::Stop)
    ));
}

#[tokio::test]
async fn parse_stream_emits_tool_call_deltas() {
    let adapter = AnthropicAdapter;
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_tool\",\"model\":\"claude-3-5-sonnet-20241022\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_123\",\"name\":\"get_weather\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\\\"Paris\\\"}\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"input_tokens\":10,\"output_tokens\":3},\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "claude-3-5-sonnet-20241022")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    let start_tool = chunks
        .iter()
        .find_map(|chunk| chunk.choices[0].delta.tool_calls.as_ref())
        .expect("expected tool call chunk");
    assert_eq!(
        start_tool[0].function.as_ref().unwrap().name.as_deref(),
        Some("get_weather")
    );
    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0]
            .delta
            .tool_calls
            .as_ref()
            .and_then(|tool_calls| tool_calls[0].function.as_ref())
            .and_then(|function| function.arguments.as_deref())
            == Some("{\"city\":\"Paris\"}")
    }));
    let final_chunk = chunks
        .iter()
        .find(|chunk| chunk.usage.is_some())
        .expect("expected usage chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::ToolCalls)
    ));
}

#[tokio::test]
async fn parse_stream_returns_error_for_anthropic_error_event() {
    let adapter = AnthropicAdapter;
    let sse_body = concat!(
        "event: error\n",
        "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"upstream overloaded\"}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let results = adapter
        .parse_chat_stream(response, "claude-3-5-sonnet-20241022")
        .unwrap()
        .collect::<Vec<_>>()
        .await;

    let error = results
        .into_iter()
        .find_map(Result::err)
        .expect("expected anthropic stream error");
    let stream_error = error
        .downcast_ref::<super::super::ProviderStreamError>()
        .expect("expected provider stream error");
    assert_eq!(stream_error.info.kind, ProviderErrorKind::Server);
    assert_eq!(stream_error.info.code, "overloaded_error");
    assert_eq!(stream_error.info.message, "upstream overloaded");
    assert!(
        error
            .to_string()
            .contains("anthropic stream error [overloaded_error]")
    );
    assert!(
        error
            .chain()
            .any(|cause| cause.to_string().contains("upstream overloaded"))
    );
}

#[tokio::test]
async fn parse_stream_does_not_emit_terminal_chunk_for_intermediate_message_delta() {
    let adapter = AnthropicAdapter;
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_partial\",\"model\":\"claude-3-5-sonnet-20241022\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"input_tokens\":12,\"output_tokens\":3},\"delta\":{}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7},\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "claude-3-5-sonnet-20241022")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert_eq!(
        chunks
            .iter()
            .filter(|chunk| chunk.choices[0].finish_reason.is_some())
            .count(),
        1
    );
    let final_chunk = chunks
        .iter()
        .rfind(|chunk| chunk.choices[0].finish_reason.is_some())
        .expect("expected terminal chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::Stop)
    ));
}

#[tokio::test]
async fn parse_stream_maps_refusal_finish_reason_to_content_filter() {
    let adapter = AnthropicAdapter;
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_refusal\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"I can’t help with that.\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7},\"delta\":{\"stop_reason\":\"refusal\"}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_chat_stream(response, "claude-sonnet-4-20250514")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    let final_chunk = chunks
        .iter()
        .rfind(|chunk| chunk.choices[0].finish_reason.is_some())
        .expect("expected terminal chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::ContentFilter)
    ));
}

#[test]
fn parse_error_maps_authentication_error_to_authentication() {
    let info = AnthropicAdapter.parse_error(
        StatusCode::UNAUTHORIZED.as_u16(),
        &HeaderMap::new(),
        br#"{"type":"error","error":{"type":"authentication_error","message":"invalid api key"}}"#,
    );

    assert_eq!(info.kind, ProviderErrorKind::Authentication);
    assert_eq!(info.code, "authentication_error");
    assert_eq!(info.message, "invalid api key");
}

#[test]
fn parse_error_maps_overloaded_error_to_server() {
    let info = AnthropicAdapter.parse_error(
        StatusCode::SERVICE_UNAVAILABLE.as_u16(),
        &HeaderMap::new(),
        br#"{"type":"error","error":{"type":"overloaded_error","message":"upstream overloaded"}}"#,
    );

    assert_eq!(info.kind, ProviderErrorKind::Server);
    assert_eq!(info.code, "overloaded_error");
    assert_eq!(info.message, "upstream overloaded");
}
