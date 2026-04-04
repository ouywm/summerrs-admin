use crate::service::openai_completions_relay::{
    bridge_chat_completion_to_completion, completion_request_to_chat_request,
};
use crate::service::openai_http::bridge_chat_completion_to_response;
use crate::service::openai_http::insert_upstream_request_id_header;
use crate::service::openai_relay_support::{
    BufferedMultipartField, build_audio_speech_request, build_batch_create_request,
    build_completion_request, build_file_upload_request, build_image_generation_request,
    build_image_variation_request, build_moderation_request, extend_limited_buffer,
    join_upstream_url, parse_audio_transcription_meta, parse_image_edit_meta,
    parse_image_variation_meta,
};
use summer_ai_core::types::audio::AudioSpeechRequest;
use summer_ai_core::types::batch::BatchCreateRequest;
use summer_ai_core::types::chat::ChatCompletionResponse;
use summer_ai_core::types::completion::{CompletionRequest, CompletionResponse};
use summer_ai_core::types::file::FileObject;
use summer_ai_core::types::image::ImageGenerationRequest;
use summer_ai_core::types::moderation::ModerationRequest;
use summer_web::axum::body::Body;
use summer_web::axum::response::Response;

#[test]
fn build_image_generation_request_targets_openai_images_endpoint() {
    let client = reqwest::Client::new();
    let req: ImageGenerationRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-image-1",
        "prompt": "draw a fox"
    }))
    .unwrap();

    let built = build_image_generation_request(
        &client,
        "https://api.example.com/",
        "sk-test",
        &req,
        "mapped-image-model",
    )
    .unwrap()
    .build()
    .unwrap();

    assert_eq!(
        built.url().as_str(),
        "https://api.example.com/v1/images/generations"
    );
    let body: serde_json::Value =
        serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["model"], "mapped-image-model");
}

#[test]
fn build_audio_speech_request_targets_openai_audio_endpoint() {
    let client = reqwest::Client::new();
    let req: AudioSpeechRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-4o-mini-tts",
        "input": "hello",
        "voice": "alloy"
    }))
    .unwrap();

    let built = build_audio_speech_request(
        &client,
        "https://api.example.com/",
        "sk-test",
        &req,
        "mapped-tts-model",
    )
    .unwrap()
    .build()
    .unwrap();

    assert_eq!(
        built.url().as_str(),
        "https://api.example.com/v1/audio/speech"
    );
    let body: serde_json::Value =
        serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["model"], "mapped-tts-model");
}

#[test]
fn insert_upstream_request_id_header_sets_header() {
    let mut response = Response::new(Body::empty());

    insert_upstream_request_id_header(&mut response, "up_req_123");

    assert_eq!(
        response
            .headers()
            .get("x-upstream-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("up_req_123")
    );
}

#[test]
fn insert_upstream_request_id_header_skips_empty_value() {
    let mut response = Response::new(Body::empty());

    insert_upstream_request_id_header(&mut response, "");

    assert!(response.headers().get("x-upstream-request-id").is_none());
}

#[test]
fn parse_audio_transcription_meta_reads_model_and_response_format() {
    let fields = vec![
        BufferedMultipartField {
            name: "model".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("whisper-1"),
        },
        BufferedMultipartField {
            name: "response_format".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("verbose_json"),
        },
        BufferedMultipartField {
            name: "prompt".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("hello world"),
        },
    ];

    let meta = parse_audio_transcription_meta(&fields).unwrap();
    assert_eq!(meta.model, "whisper-1");
    assert_eq!(meta.response_format.as_deref(), Some("verbose_json"));
    assert_eq!(meta.estimated_tokens, 3);
}

#[test]
fn parse_image_edit_meta_reads_model_and_prompt() {
    let fields = vec![
        BufferedMultipartField {
            name: "model".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("gpt-image-1"),
        },
        BufferedMultipartField {
            name: "prompt".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("make the sky blue"),
        },
    ];

    let meta = parse_image_edit_meta(&fields).unwrap();
    assert_eq!(meta.model, "gpt-image-1");
    assert_eq!(meta.estimated_tokens, 5);
}

#[test]
fn parse_image_variation_meta_reads_model_and_image_count() {
    let fields = vec![
        BufferedMultipartField {
            name: "model".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("dall-e-2"),
        },
        BufferedMultipartField {
            name: "n".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("3"),
        },
        BufferedMultipartField {
            name: "image".into(),
            filename: Some("otter.png".into()),
            content_type: Some("image/png".into()),
            bytes: bytes::Bytes::from_static(b"png-bytes"),
        },
    ];

    let meta = parse_image_variation_meta(&fields).unwrap();
    assert_eq!(meta.model, "dall-e-2");
    assert_eq!(meta.estimated_tokens, 3);
}

#[test]
fn build_image_variation_request_targets_openai_variations_endpoint() {
    let client = reqwest::Client::new();
    let fields = vec![
        BufferedMultipartField {
            name: "model".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("dall-e-2"),
        },
        BufferedMultipartField {
            name: "image".into(),
            filename: Some("otter.png".into()),
            content_type: Some("image/png".into()),
            bytes: bytes::Bytes::from_static(b"png-bytes"),
        },
    ];

    let built = build_image_variation_request(
        &client,
        "https://api.example.com/",
        "sk-test",
        &fields,
        "mapped-image-model",
    )
    .unwrap()
    .build()
    .unwrap();

    assert_eq!(
        built.url().as_str(),
        "https://api.example.com/v1/images/variations"
    );
}

#[test]
fn build_moderation_request_targets_openai_moderations_endpoint() {
    let client = reqwest::Client::new();
    let req: ModerationRequest = serde_json::from_value(serde_json::json!({
        "model": "omni-moderation-latest",
        "input": "hello"
    }))
    .unwrap();

    let built = build_moderation_request(
        &client,
        "https://api.example.com/",
        "sk-test",
        &req,
        "mapped-moderation-model",
    )
    .unwrap()
    .build()
    .unwrap();

    assert_eq!(
        built.url().as_str(),
        "https://api.example.com/v1/moderations"
    );
    let body: serde_json::Value =
        serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["model"], "mapped-moderation-model");
}

#[test]
fn build_file_upload_request_targets_openai_files_endpoint() {
    let client = reqwest::Client::new();
    let fields = vec![
        BufferedMultipartField {
            name: "purpose".into(),
            filename: None,
            content_type: None,
            bytes: bytes::Bytes::from("assistants"),
        },
        BufferedMultipartField {
            name: "file".into(),
            filename: Some("notes.txt".into()),
            content_type: Some("text/plain".into()),
            bytes: bytes::Bytes::from_static(b"hello"),
        },
    ];

    let built = build_file_upload_request(&client, "https://api.example.com/", "sk-test", &fields)
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(built.url().as_str(), "https://api.example.com/v1/files");
}

#[test]
fn build_completion_request_targets_openai_completions_endpoint() {
    let client = reqwest::Client::new();
    let req: CompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-3.5-turbo-instruct",
        "prompt": "hello"
    }))
    .unwrap();

    let built = build_completion_request(
        &client,
        "https://api.example.com/",
        "sk-test",
        &req,
        "mapped-completion-model",
    )
    .unwrap()
    .build()
    .unwrap();

    assert_eq!(
        built.url().as_str(),
        "https://api.example.com/v1/completions"
    );
    let body: serde_json::Value =
        serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["model"], "mapped-completion-model");
}

#[test]
fn build_completion_request_enables_usage_chunks_for_stream() {
    let client = reqwest::Client::new();
    let req: CompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-3.5-turbo-instruct",
        "prompt": "hello",
        "stream": true
    }))
    .unwrap();

    let built = build_completion_request(
        &client,
        "https://api.example.com/",
        "sk-test",
        &req,
        "mapped-completion-model",
    )
    .unwrap()
    .build()
    .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["stream_options"]["include_usage"], true);
}

#[test]
fn join_upstream_url_preserves_response_subresource_path() {
    let url = join_upstream_url(
        "https://api.example.com/",
        "/v1/responses/resp_123/input_items",
    )
    .unwrap();

    assert_eq!(
        url.as_str(),
        "https://api.example.com/v1/responses/resp_123/input_items"
    );
}

#[test]
fn build_batch_create_request_targets_openai_batches_endpoint() {
    let client = reqwest::Client::new();
    let req: BatchCreateRequest = serde_json::from_value(serde_json::json!({
        "input_file_id": "file_123",
        "endpoint": "/v1/chat/completions",
        "completion_window": "24h"
    }))
    .unwrap();

    let built = build_batch_create_request(&client, "https://api.example.com/", "sk-test", &req)
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(built.url().as_str(), "https://api.example.com/v1/batches");
}

#[test]
fn file_object_deserializes() {
    let file: FileObject = serde_json::from_value(serde_json::json!({
        "id": "file-123",
        "object": "file",
        "bytes": 5,
        "created_at": 1700000000,
        "filename": "notes.txt",
        "purpose": "assistants"
    }))
    .unwrap();

    assert_eq!(file.id, "file-123");
    assert_eq!(file.filename, "notes.txt");
}

#[test]
fn bridge_chat_completion_to_response_preserves_usage_and_text() {
    let response: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
        "id": "chatcmpl_123",
        "object": "chat.completion",
        "created": 1_774_427_062,
        "model": "claude-sonnet-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello from bridge"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 7,
            "total_tokens": 19,
            "cached_tokens": 3,
            "reasoning_tokens": 5
        }
    }))
    .unwrap();

    let bridged = bridge_chat_completion_to_response(response);
    assert_eq!(bridged.id, "chatcmpl_123");
    assert_eq!(bridged.object, "response");
    assert_eq!(bridged.model, "claude-sonnet-4");
    assert_eq!(bridged.status, "completed");
    assert_eq!(bridged.output_text.as_deref(), Some("Hello from bridge"));
    assert_eq!(
        bridged.usage.as_ref().map(|usage| usage.total_tokens),
        Some(19)
    );
    assert_eq!(
        bridged
            .usage
            .as_ref()
            .and_then(|usage| usage.input_tokens_details.as_ref())
            .map(|details| details.cached_tokens),
        Some(3)
    );
    assert_eq!(
        bridged
            .usage
            .as_ref()
            .and_then(|usage| usage.output_tokens_details.as_ref())
            .map(|details| details.reasoning_tokens),
        Some(5)
    );
}

#[test]
fn completion_request_to_chat_request_maps_prompt_and_sampling_fields() {
    let request: CompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-3.5-turbo-instruct",
        "prompt": ["hello", "world"],
        "stream": true,
        "max_tokens": 256,
        "temperature": 0.7,
        "top_p": 0.9,
        "presence_penalty": 0.2,
        "frequency_penalty": 0.1,
        "stop": ["END"],
        "user": "demo-user"
    }))
    .unwrap();

    let bridged = completion_request_to_chat_request(&request).unwrap();
    assert_eq!(bridged.model, "gpt-3.5-turbo-instruct");
    assert_eq!(bridged.messages.len(), 1);
    assert_eq!(bridged.messages[0].role, "user");
    assert_eq!(
        bridged.messages[0].content,
        serde_json::json!("hello\nworld")
    );
    assert!(bridged.stream);
    assert_eq!(bridged.max_tokens, Some(256));
    assert_eq!(bridged.temperature, Some(0.7));
    assert_eq!(bridged.top_p, Some(0.9));
    assert_eq!(bridged.presence_penalty, Some(0.2));
    assert_eq!(bridged.frequency_penalty, Some(0.1));
    assert_eq!(bridged.stop, Some(serde_json::json!(["END"])));
    assert_eq!(
        bridged.extra.get("user"),
        Some(&serde_json::json!("demo-user"))
    );
}

#[test]
fn bridge_chat_completion_to_completion_preserves_usage_and_text() {
    let response: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
        "id": "chatcmpl_completion_123",
        "object": "chat.completion",
        "created": 1_774_427_062,
        "model": "claude-sonnet-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello from completion bridge"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 7,
            "total_tokens": 19
        }
    }))
    .unwrap();

    let bridged: CompletionResponse = bridge_chat_completion_to_completion(response);
    assert_eq!(bridged.id, "chatcmpl_completion_123");
    assert_eq!(bridged.object, "text_completion");
    assert_eq!(bridged.model, "claude-sonnet-4");
    assert_eq!(bridged.choices.len(), 1);
    assert_eq!(bridged.choices[0].text, "Hello from completion bridge");
    assert_eq!(
        serde_json::to_value(&bridged).unwrap()["usage"]["total_tokens"],
        serde_json::json!(19)
    );
}

#[test]
fn chat_relay_impl_is_not_defined_in_router_module() {
    let source = route_source("openai/mod.rs");

    assert!(
        !source.contains("pub(crate) async fn relay_chat_completions_impl("),
        "chat relay implementation should live in a service module, not router/openai/mod.rs"
    );
}

#[test]
fn endpoint_route_modules_only_keep_wrappers() {
    assert_route_module_omits(
        "openai/completions.rs",
        "fn completion_request_to_chat_request(",
        "completions route should delegate to a service module",
    );
    assert_route_module_omits(
        "openai/audio.rs",
        "fn build_audio_speech_request_for_channel(",
        "audio route should delegate to a service module",
    );
    assert_route_module_omits(
        "openai/audio_transcribe.rs",
        "async fn relay_audio_multipart_request(",
        "audio multipart route should delegate to a service module",
    );
    assert_route_module_omits(
        "openai/moderations.rs",
        "fn build_moderation_request_for_channel(",
        "moderations route should delegate to a service module",
    );
    assert_route_module_omits(
        "openai/rerank.rs",
        "fn build_rerank_request_for_channel(",
        "rerank route should delegate to a service module",
    );
    assert_route_module_omits(
        "openai/images.rs",
        "fn build_image_generation_request_for_channel(",
        "images route should delegate to a service module",
    );
    assert_route_module_omits(
        "openai/image_multipart.rs",
        "async fn relay_image_multipart_request(",
        "image multipart route should delegate to a service module",
    );
}

#[test]
fn openai_router_module_does_not_reexport_test_helpers() {
    let source = route_source("openai/mod.rs");

    assert!(
        !source.contains("pub(crate) use crate::service::openai_completions_relay::"),
        "router/openai/mod.rs should not re-export completion helpers for tests"
    );
    assert!(
        !source.contains("pub(crate) use crate::service::openai_relay_support::*;"),
        "router/openai/mod.rs should not re-export relay support helpers for tests"
    );
    assert!(
        !source.contains("pub(crate) use crate::service::openai_tracking::"),
        "router/openai/mod.rs should not re-export tracking helpers for tests"
    );
}

fn route_source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/router/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|_| panic!("read src/router/{path}"))
}

fn assert_route_module_omits(path: &str, needle: &str, message: &str) {
    let source = route_source(path);
    assert!(!source.contains(needle), "{message}");
}

#[test]
fn extend_limited_buffer_rejects_payload_over_limit() {
    let mut buffer = Vec::new();
    let error = extend_limited_buffer(&mut buffer, b"abcdef", 4, "file").unwrap_err();

    assert_eq!(
        error.status,
        summer_web::axum::http::StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(error.error.error.code.as_deref(), Some("payload_too_large"));
}

#[test]
fn extend_limited_buffer_accepts_payload_within_limit() {
    let mut buffer = Vec::new();

    extend_limited_buffer(&mut buffer, b"ab", 4, "file").unwrap();
    extend_limited_buffer(&mut buffer, b"cd", 4, "file").unwrap();

    assert_eq!(buffer, b"abcd");
}

mod mock_upstream;
