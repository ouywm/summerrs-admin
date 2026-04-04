use super::*;
use futures::stream;
use reqwest::StatusCode;

fn sample_request() -> ChatCompletionRequest {
    serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"}
        ]
    }))
    .unwrap()
}

#[test]
fn build_request_targets_generate_content_endpoint() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let builder = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &sample_request(),
            "gemini-2.5-pro",
        )
        .unwrap();

    let request = builder.build().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
    );
    assert_eq!(request.headers().get("x-goog-api-key").unwrap(), "gem-key");
}

#[test]
fn build_stream_request_targets_stream_generate_content_sse_endpoint() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let mut request = sample_request();
    request.stream = true;

    let builder = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &request,
            "gemini-2.5-pro",
        )
        .unwrap();

    let request = builder.build().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse"
    );
    assert_eq!(request.headers().get("x-goog-api-key").unwrap(), "gem-key");
}

#[test]
fn build_request_respects_explicit_v1_base_url() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let builder = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com/v1",
            "gem-key",
            &sample_request(),
            "gemini-2.5-pro",
        )
        .unwrap();

    let request = builder.build().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://generativelanguage.googleapis.com/v1/models/gemini-2.5-pro:generateContent"
    );
}

#[test]
fn build_embeddings_request_targets_embed_content_endpoint() {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "model": "text-embedding-004",
        "input": "hello",
        "dimensions": 128
    });

    let request = GeminiAdapter
        .build_embeddings_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &payload,
            "text-embedding-004",
        )
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        request.url().as_str(),
        "https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent"
    );
    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["model"],
        serde_json::json!("models/text-embedding-004")
    );
    assert_eq!(
        body["content"]["parts"][0]["text"],
        serde_json::json!("hello")
    );
    assert_eq!(body["outputDimensionality"], serde_json::json!(128));
}

#[test]
fn build_embeddings_request_targets_batch_embed_contents_endpoint() {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "model": "text-embedding-004",
        "input": ["hello", "world"],
        "taskType": "RETRIEVAL_DOCUMENT"
    });

    let request = GeminiAdapter
        .build_embeddings_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &payload,
            "text-embedding-004",
        )
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        request.url().as_str(),
        "https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:batchEmbedContents"
    );
    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["requests"][0]["model"],
        serde_json::json!("models/text-embedding-004")
    );
    assert_eq!(
        body["requests"][0]["content"]["parts"][0]["text"],
        serde_json::json!("hello")
    );
    assert_eq!(
        body["requests"][1]["content"]["parts"][0]["text"],
        serde_json::json!("world")
    );
    assert_eq!(
        body["requests"][0]["taskType"],
        serde_json::json!("RETRIEVAL_DOCUMENT")
    );
}

#[test]
fn build_request_converts_data_url_image_to_inline_data() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "Describe this image"},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "data:image/png;base64,aGVsbG8="
                    }
                }
            ]
        }]
    }))
    .unwrap();

    let request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["contents"][0]["parts"][1],
        serde_json::json!({
            "inlineData": {
                "mimeType": "image/png",
                "data": "aGVsbG8="
            }
        })
    );
}

#[test]
fn build_request_converts_file_uri_image_to_file_data() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [{
            "role": "user",
            "content": [{
                "type": "image_url",
                "image_url": {
                    "url": "https://generativelanguage.googleapis.com/v1beta/files/file-123",
                    "mime_type": "image/png"
                }
            }]
        }]
    }))
    .unwrap();

    let request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["contents"][0]["parts"][0]["fileData"],
        serde_json::json!({
            "mimeType": "image/png",
            "fileUri": "https://generativelanguage.googleapis.com/v1beta/files/file-123"
        })
    );
}

#[test]
fn build_request_maps_required_tool_choice_to_any_mode() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [{"role": "user", "content": "weather?"}],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather info",
                "parameters": {"type": "object"}
            }
        }],
        "tool_choice": "required"
    }))
    .unwrap();

    let request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["toolConfig"]["functionCallingConfig"]["mode"], "ANY");
}

#[test]
fn build_request_maps_specific_tool_choice_to_allowed_function_names() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [{"role": "user", "content": "weather?"}],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather info",
                "parameters": {"type": "object"}
            }
        }],
        "tool_choice": {
            "type": "function",
            "function": {"name": "get_weather"}
        }
    }))
    .unwrap();

    let request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["toolConfig"]["functionCallingConfig"],
        serde_json::json!({
            "mode": "ANY",
            "allowedFunctionNames": ["get_weather"]
        })
    );
}

#[test]
fn build_request_preserves_safety_settings_extra_body_fields() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [{"role": "user", "content": "hello"}],
        "safetySettings": [{
            "category": "HARM_CATEGORY_HATE_SPEECH",
            "threshold": "BLOCK_ONLY_HIGH"
        }]
    }))
    .unwrap();

    let request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["safetySettings"],
        serde_json::json!([{
            "category": "HARM_CATEGORY_HATE_SPEECH",
            "threshold": "BLOCK_ONLY_HIGH"
        }])
    );
}

#[test]
fn build_request_preserves_structured_function_response_payload() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"city\":\"Paris\"}"
                    }
                }]
            },
            {
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "{\"temperatureC\":15,\"condition\":\"sunny\"}"
            }
        ]
    }))
    .unwrap();

    let request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["contents"][1]["parts"][0]["functionResponse"],
        serde_json::json!({
            "name": "get_weather",
            "response": {
                "temperatureC": 15,
                "condition": "sunny"
            }
        })
    );
}

#[test]
fn build_request_sets_system_instruction_and_generation_config() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [
            {"role": "system", "content": "Always answer in JSON."},
            {"role": "user", "content": "Describe Paris weather"}
        ],
        "temperature": 0.2,
        "top_p": 0.7,
        "max_tokens": 256,
        "stop": ["END"],
        "response_format": {
            "type": "json_object"
        }
    }))
    .unwrap();

    let request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["systemInstruction"],
        serde_json::json!({
            "parts": [{
                "text": "Always answer in JSON."
            }]
        })
    );
    assert_eq!(
        body["generationConfig"],
        serde_json::json!({
            "temperature": 0.2,
            "topP": 0.7,
            "maxOutputTokens": 256,
            "stopSequences": ["END"],
            "responseMimeType": "application/json"
        })
    );
}

#[test]
fn build_request_maps_json_schema_response_format_to_response_json_schema() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [{"role": "user", "content": "Return a person object"}],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "person",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "age": {"type": "integer"}
                    },
                    "required": ["name", "age"]
                }
            }
        }
    }))
    .unwrap();

    let request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        body["generationConfig"]["responseMimeType"],
        serde_json::json!("application/json")
    );
    assert_eq!(
        body["generationConfig"]["responseJsonSchema"],
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"]
        })
    );
}

#[test]
fn build_request_maps_auto_and_none_tool_choice_modes() {
    let client = reqwest::Client::new();
    let adapter = GeminiAdapter;
    let auto_req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [{"role": "user", "content": "hello"}],
        "tool_choice": "auto"
    }))
    .unwrap();
    let none_req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gemini-2.5-pro",
        "messages": [{"role": "user", "content": "hello"}],
        "tool_choice": "none"
    }))
    .unwrap();

    let auto_request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &auto_req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();
    let none_request = adapter
        .build_request(
            &client,
            "https://generativelanguage.googleapis.com",
            "gem-key",
            &none_req,
            "gemini-2.5-pro",
        )
        .unwrap()
        .build()
        .unwrap();

    let auto_body: serde_json::Value =
        serde_json::from_slice(auto_request.body().unwrap().as_bytes().unwrap()).unwrap();
    let none_body: serde_json::Value =
        serde_json::from_slice(none_request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(
        auto_body["toolConfig"]["functionCallingConfig"]["mode"],
        serde_json::json!("AUTO")
    );
    assert_eq!(
        none_body["toolConfig"]["functionCallingConfig"]["mode"],
        serde_json::json!("NONE")
    );
}

#[test]
fn parse_response_converts_text_and_usage() {
    let adapter = GeminiAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello from Gemini"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 4,
                "candidatesTokenCount": 6,
                "totalTokenCount": 10
            }
        }))
        .unwrap(),
    );

    let response = adapter.parse_response(body, "gemini-2.5-pro").unwrap();
    assert_eq!(response.model, "gemini-2.5-pro");
    assert_eq!(
        response.choices[0].message.content,
        serde_json::Value::String("Hello from Gemini".into())
    );
    assert_eq!(response.usage.total_tokens, 10);
}

#[test]
fn parse_embeddings_response_converts_single_embedding() {
    let adapter = GeminiAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "embedding": {
                "values": [1.0, 2.0]
            }
        }))
        .unwrap(),
    );

    let response = adapter
        .parse_embeddings_response(body, "text-embedding-004", 8)
        .unwrap();
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].index, 0);
    assert_eq!(response.data[0].embedding, serde_json::json!([1.0, 2.0]));
    assert_eq!(response.usage.total_tokens, 8);
}

#[test]
fn parse_embeddings_response_converts_batch_embeddings() {
    let adapter = GeminiAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "embeddings": [
                {"values": [1.0, 2.0]},
                {"values": [3.0, 4.0]}
            ]
        }))
        .unwrap(),
    );

    let response = adapter
        .parse_embeddings_response(body, "text-embedding-004", 12)
        .unwrap();
    assert_eq!(response.data.len(), 2);
    assert_eq!(response.data[0].embedding, serde_json::json!([1.0, 2.0]));
    assert_eq!(response.data[1].embedding, serde_json::json!([3.0, 4.0]));
    assert_eq!(response.usage.total_tokens, 12);
}

#[test]
fn parse_response_returns_multiple_choices_for_multiple_candidates() {
    let adapter = GeminiAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "candidates": [
                {
                    "content": {
                        "parts": [{"text": "Hello from Gemini"}]
                    },
                    "finishReason": "STOP"
                },
                {
                    "content": {
                        "parts": [{"text": "Need more room"}]
                    },
                    "finishReason": "MAX_TOKENS"
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 4,
                "candidatesTokenCount": 6,
                "totalTokenCount": 10
            }
        }))
        .unwrap(),
    );

    let response = adapter.parse_response(body, "gemini-2.5-pro").unwrap();
    assert_eq!(response.choices.len(), 2);
    assert_eq!(
        response.choices[0].message.content,
        serde_json::Value::String("Hello from Gemini".into())
    );
    assert_eq!(
        response.choices[1].message.content,
        serde_json::Value::String("Need more room".into())
    );
    assert!(matches!(
        response.choices[0].finish_reason,
        Some(FinishReason::Stop)
    ));
    assert!(matches!(
        response.choices[1].finish_reason,
        Some(FinishReason::Length)
    ));
}

#[test]
fn parse_response_converts_function_call_candidate() {
    let adapter = GeminiAdapter;
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "get_weather",
                            "args": {"city": "Paris"}
                        }
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 4,
                "candidatesTokenCount": 2,
                "totalTokenCount": 6
            }
        }))
        .unwrap(),
    );

    let response = adapter.parse_response(body, "gemini-2.5-pro").unwrap();
    let tool_calls = response.choices[0].message.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].function.name, "get_weather");
    assert_eq!(tool_calls[0].function.arguments, "{\"city\":\"Paris\"}");
    assert!(matches!(
        response.choices[0].finish_reason,
        Some(FinishReason::ToolCalls)
    ));
    assert_eq!(response.usage.total_tokens, 6);
}

#[tokio::test]
async fn parse_stream_handles_multiline_sse_and_usage_only_terminal_event() {
    let adapter = GeminiAdapter;
    let sse_body = concat!(
        "data: {\"candidates\":[\n",
        "data: {\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}\n",
        "data: ]}\n\n",
        "data: {\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6,\"totalTokenCount\":10}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_stream(response, "gemini-2.5-pro")
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
        chunks
            .iter()
            .any(|chunk| chunk.choices[0].delta.content.as_deref() == Some("Hello"))
    );
    let usage_chunk = chunks
        .iter()
        .find(|chunk| chunk.usage.is_some())
        .expect("expected usage chunk");
    assert_eq!(
        usage_chunk.usage.as_ref().map(|usage| usage.total_tokens),
        Some(10)
    );
    assert!(usage_chunk.choices[0].finish_reason.is_none());
}

#[tokio::test]
async fn parse_stream_preserves_utf8_when_sse_chunk_splits_multibyte_boundary() {
    let adapter = GeminiAdapter;
    let event =
        concat!("data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"你好\"}]}}]}\n\n");
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
        .parse_stream(response, "gemini-2.5-pro")
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
async fn parse_stream_emits_choice_indexes_for_multiple_candidates() {
    let adapter = GeminiAdapter;
    let sse_body = concat!(
        "data: {\"candidates\":[",
        "{\"content\":{\"parts\":[{\"text\":\"Hello\"}]},\"finishReason\":\"STOP\"},",
        "{\"content\":{\"parts\":[{\"text\":\"Bonjour\"}]},\"finishReason\":\"MAX_TOKENS\"}",
        "],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6,\"totalTokenCount\":10}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_stream(response, "gemini-2.5-pro")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].index == 0 && chunk.choices[0].delta.content.as_deref() == Some("Hello")
    }));
    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].index == 1 && chunk.choices[0].delta.content.as_deref() == Some("Bonjour")
    }));
    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].index == 0
            && matches!(chunk.choices[0].finish_reason, Some(FinishReason::Stop))
    }));
    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].index == 1
            && matches!(chunk.choices[0].finish_reason, Some(FinishReason::Length))
    }));
}

#[tokio::test]
async fn parse_stream_emits_usage_only_once_for_multiple_candidates() {
    let adapter = GeminiAdapter;
    let sse_body = concat!(
        "data: {\"candidates\":[",
        "{\"content\":{\"parts\":[{\"text\":\"Hello\"}]},\"finishReason\":\"STOP\"},",
        "{\"content\":{\"parts\":[{\"text\":\"Bonjour\"}]},\"finishReason\":\"MAX_TOKENS\"}",
        "],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6,\"totalTokenCount\":10}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_stream(response, "gemini-2.5-pro")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    let usage_chunks = chunks
        .iter()
        .filter(|chunk| chunk.usage.is_some())
        .collect::<Vec<_>>();
    assert_eq!(usage_chunks.len(), 1);
    assert_eq!(
        usage_chunks[0]
            .usage
            .as_ref()
            .map(|usage| usage.total_tokens),
        Some(10)
    );
}

#[tokio::test]
async fn parse_stream_emits_text_and_usage() {
    let adapter = GeminiAdapter;
    let sse_body = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6,\"totalTokenCount\":10}}\n\n";

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_stream(response, "gemini-2.5-pro")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert_eq!(
        chunks[0].choices[0].delta.role.as_deref(),
        Some("assistant")
    );
    assert_eq!(chunks[1].choices[0].delta.content.as_deref(), Some("Hello"));
    assert_eq!(
        chunks[2].usage.as_ref().map(|usage| usage.total_tokens),
        Some(10)
    );
    assert!(matches!(
        chunks[2].choices[0].finish_reason,
        Some(FinishReason::Stop)
    ));
}

#[tokio::test]
async fn parse_stream_emits_function_call_deltas() {
    let adapter = GeminiAdapter;
    let sse_body = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Paris\"}}}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2,\"totalTokenCount\":6}}\n\n";

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_stream(response, "gemini-2.5-pro")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    let tool_calls = chunks[1].choices[0].delta.tool_calls.as_ref().unwrap();
    assert_eq!(
        tool_calls[0].function.as_ref().unwrap().name.as_deref(),
        Some("get_weather")
    );
    assert_eq!(
        tool_calls[0]
            .function
            .as_ref()
            .unwrap()
            .arguments
            .as_deref(),
        Some("{\"city\":\"Paris\"}")
    );
    assert!(matches!(
        chunks[2].choices[0].finish_reason,
        Some(FinishReason::ToolCalls)
    ));
}

#[tokio::test]
async fn parse_stream_keeps_tool_call_finish_reason_across_events() {
    let adapter = GeminiAdapter;
    let sse_body = concat!(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Paris\"}}}]}}]}\n\n",
        "data: {\"candidates\":[{\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2,\"totalTokenCount\":6}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_stream(response, "gemini-2.5-pro")
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
        Some(FinishReason::ToolCalls)
    ));
}

#[tokio::test]
async fn parse_stream_reuses_tool_call_index_across_events() {
    let adapter = GeminiAdapter;
    let sse_body = concat!(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Par\"}}}]}}]}\n\n",
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Paris\"}}}]}}]}\n\n",
        "data: {\"candidates\":[{\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2,\"totalTokenCount\":6}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_stream(response, "gemini-2.5-pro")
        .unwrap()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    let tool_call_chunks = chunks
        .iter()
        .filter_map(|chunk| chunk.choices[0].delta.tool_calls.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(tool_call_chunks.len(), 2);
    assert_eq!(tool_call_chunks[0][0].index, 0);
    assert_eq!(tool_call_chunks[1][0].index, 0);
    assert_eq!(tool_call_chunks[0][0].id.as_deref(), Some("call_0"));
    assert_eq!(tool_call_chunks[1][0].id.as_deref(), Some("call_0"));

    let final_chunk = chunks
        .iter()
        .rfind(|chunk| chunk.choices[0].finish_reason.is_some())
        .expect("expected terminal chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::ToolCalls)
    ));
}

#[tokio::test]
async fn parse_stream_does_not_emit_terminal_chunk_before_finish_reason() {
    let adapter = GeminiAdapter;
    let sse_body = concat!(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}\n\n",
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"lo\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2,\"totalTokenCount\":6}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let chunks: Vec<_> = adapter
        .parse_stream(response, "gemini-2.5-pro")
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
        .expect("expected final terminal chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(FinishReason::Stop)
    ));
}

#[tokio::test]
async fn parse_stream_returns_error_for_gemini_error_event() {
    let adapter = GeminiAdapter;
    let sse_body = concat!(
        "event: error\n",
        "data: {\"error\":{\"status\":\"INVALID_ARGUMENT\",\"message\":\"bad tool schema\"}}\n\n"
    );

    let mock_response = http::Response::builder()
        .status(200)
        .body(sse_body.to_string())
        .unwrap();
    let response = reqwest::Response::from(mock_response);

    let results = adapter
        .parse_stream(response, "gemini-2.5-pro")
        .unwrap()
        .collect::<Vec<_>>()
        .await;

    let error = results
        .into_iter()
        .find_map(Result::err)
        .expect("expected gemini stream error");
    let stream_error = error
        .downcast_ref::<super::super::ProviderStreamError>()
        .expect("expected provider stream error");
    assert_eq!(stream_error.info.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(stream_error.info.code, "INVALID_ARGUMENT");
    assert_eq!(stream_error.info.message, "bad tool schema");
    assert!(
        error
            .to_string()
            .contains("gemini stream error [INVALID_ARGUMENT]")
    );
    assert!(
        error
            .chain()
            .any(|cause| cause.to_string().contains("bad tool schema"))
    );
}

#[test]
fn convert_contents_uses_function_name_for_tool_response() {
    let messages = vec![
        Message {
            role: "assistant".into(),
            content: serde_json::Value::Null,
            name: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_123".into(),
                r#type: "function".into(),
                function: FunctionCall {
                    name: "get_weather".into(),
                    arguments: "{\"city\":\"Paris\"}".into(),
                },
            }]),
            tool_call_id: None,
        },
        Message {
            role: "tool".into(),
            content: serde_json::Value::String("sunny".into()),
            name: None,
            tool_calls: None,
            tool_call_id: Some("call_123".into()),
        },
    ];

    let contents = convert_contents(&messages);
    let tool_response = contents[1].parts[0]
        .function_response
        .as_ref()
        .expect("expected function response");
    assert_eq!(tool_response.name, "get_weather");
}

#[test]
fn parse_error_treats_failed_precondition_as_account_level_api_error() {
    let info = GeminiAdapter.parse_error(
        StatusCode::BAD_REQUEST.as_u16(),
        &HeaderMap::new(),
        br#"{"error":{"status":"FAILED_PRECONDITION","message":"project is not configured"}}"#,
    );

    assert_eq!(info.kind, ProviderErrorKind::Api);
    assert_eq!(info.code, "FAILED_PRECONDITION");
    assert_eq!(info.message, "project is not configured");
}

#[test]
fn parse_error_maps_resource_exhausted_to_rate_limit() {
    let info = GeminiAdapter.parse_error(
        StatusCode::TOO_MANY_REQUESTS.as_u16(),
        &HeaderMap::new(),
        br#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"slow down"}}"#,
    );

    assert_eq!(info.kind, ProviderErrorKind::RateLimit);
    assert_eq!(info.code, "RESOURCE_EXHAUSTED");
    assert_eq!(info.message, "slow down");
}

#[test]
fn parse_error_maps_unauthenticated_to_authentication() {
    let info = GeminiAdapter.parse_error(
        StatusCode::UNAUTHORIZED.as_u16(),
        &HeaderMap::new(),
        br#"{"error":{"status":"UNAUTHENTICATED","message":"invalid api key"}}"#,
    );

    assert_eq!(info.kind, ProviderErrorKind::Authentication);
    assert_eq!(info.code, "UNAUTHENTICATED");
    assert_eq!(info.message, "invalid api key");
}

#[test]
fn parse_error_maps_unavailable_to_server() {
    let info = GeminiAdapter.parse_error(
        StatusCode::SERVICE_UNAVAILABLE.as_u16(),
        &HeaderMap::new(),
        br#"{"error":{"status":"UNAVAILABLE","message":"upstream overloaded"}}"#,
    );

    assert_eq!(info.kind, ProviderErrorKind::Server);
    assert_eq!(info.code, "UNAVAILABLE");
    assert_eq!(info.message, "upstream overloaded");
}
