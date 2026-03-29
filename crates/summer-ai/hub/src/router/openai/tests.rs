use super::*;
use summer_ai_core::types::audio::AudioSpeechRequest;
use summer_ai_core::types::batch::BatchCreateRequest;
use summer_ai_core::types::chat::ChatCompletionResponse;
use summer_ai_core::types::completion::CompletionRequest;
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

mod mock_upstream;
