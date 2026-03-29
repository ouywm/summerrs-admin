#![allow(dead_code)]

use anyhow::{Context, anyhow};
use bytes::Bytes;
use reqwest::Url;
use reqwest::multipart::{Form, Part};
use summer_ai_core::types::audio::AudioSpeechRequest;
use summer_ai_core::types::batch::BatchCreateRequest;
use summer_ai_core::types::completion::CompletionRequest;
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_core::types::image::ImageGenerationRequest;
use summer_ai_core::types::moderation::ModerationRequest;
use summer_web::axum::extract::Multipart;
use summer_web::axum::http::{HeaderValue, StatusCode, header::CONTENT_TYPE};
use summer_web::axum::response::Response;

#[derive(Debug, Clone)]
pub(crate) struct BufferedMultipartField {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Bytes,
}

#[derive(Debug, Clone)]
pub(crate) struct AudioTranscriptionMeta {
    pub model: String,
    pub response_format: Option<String>,
    pub estimated_tokens: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct ImageEditMeta {
    pub model: String,
    pub estimated_tokens: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct ImageVariationMeta {
    pub model: String,
    pub estimated_tokens: i32,
}

pub(crate) async fn buffer_multipart_fields(
    multipart: &mut Multipart,
) -> OpenAiApiResult<Vec<BufferedMultipartField>> {
    let mut fields = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        OpenAiErrorResponse::internal_with("failed to read multipart field", error)
    })? {
        let Some(name) = field.name().map(ToOwned::to_owned) else {
            continue;
        };
        let filename = field.file_name().map(ToOwned::to_owned);
        let content_type = field.content_type().map(ToOwned::to_owned);
        let bytes = field.bytes().await.map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to buffer multipart field", error)
        })?;
        fields.push(BufferedMultipartField {
            name,
            filename,
            content_type,
            bytes,
        });
    }

    Ok(fields)
}

pub(crate) fn parse_audio_transcription_meta(
    fields: &[BufferedMultipartField],
) -> anyhow::Result<AudioTranscriptionMeta> {
    let model = required_text_field(fields, "model")?;
    let response_format = optional_text_field(fields, "response_format");
    let estimated_tokens = optional_text_field(fields, "prompt")
        .map(|prompt| estimate_text_tokens(&prompt))
        .unwrap_or(1);

    Ok(AudioTranscriptionMeta {
        model,
        response_format,
        estimated_tokens,
    })
}

pub(crate) fn parse_image_edit_meta(
    fields: &[BufferedMultipartField],
) -> anyhow::Result<ImageEditMeta> {
    let model = required_text_field(fields, "model")?;
    let estimated_tokens = optional_text_field(fields, "prompt")
        .map(|prompt| estimate_text_tokens(&prompt))
        .unwrap_or(1);

    Ok(ImageEditMeta {
        model,
        estimated_tokens,
    })
}

pub(crate) fn parse_image_variation_meta(
    fields: &[BufferedMultipartField],
) -> anyhow::Result<ImageVariationMeta> {
    let model = required_text_field(fields, "model")?;
    let estimated_tokens = optional_text_field(fields, "n")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1);

    Ok(ImageVariationMeta {
        model,
        estimated_tokens,
    })
}

pub(crate) fn build_completion_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &CompletionRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    let mut payload = json_payload_with_model(req, actual_model)?;
    if payload
        .get("stream")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        payload["stream_options"] = serde_json::json!({ "include_usage": true });
    }

    Ok(client
        .post(join_upstream_url(base_url, "/v1/completions")?)
        .bearer_auth(api_key)
        .json(&payload))
}

pub(crate) fn build_moderation_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &ModerationRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    build_json_request(
        client,
        base_url,
        api_key,
        "/v1/moderations",
        req,
        actual_model,
    )
}

pub(crate) fn build_image_generation_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &ImageGenerationRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    build_json_request(
        client,
        base_url,
        api_key,
        "/v1/images/generations",
        req,
        actual_model,
    )
}

pub(crate) fn build_audio_speech_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &AudioSpeechRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    build_json_request(
        client,
        base_url,
        api_key,
        "/v1/audio/speech",
        req,
        actual_model,
    )
}

pub(crate) fn build_batch_create_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &BatchCreateRequest,
) -> anyhow::Result<reqwest::RequestBuilder> {
    Ok(client
        .post(join_upstream_url(base_url, "/v1/batches")?)
        .bearer_auth(api_key)
        .json(req))
}

pub(crate) fn build_file_upload_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    fields: &[BufferedMultipartField],
) -> anyhow::Result<reqwest::RequestBuilder> {
    Ok(client
        .post(join_upstream_url(base_url, "/v1/files")?)
        .bearer_auth(api_key)
        .multipart(build_form(fields, None)?))
}

pub(crate) fn build_image_edit_form(
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<Form> {
    build_form(fields, Some(actual_model))
}

pub(crate) fn build_image_variation_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    Ok(client
        .post(join_upstream_url(base_url, "/v1/images/variations")?)
        .bearer_auth(api_key)
        .multipart(build_form(fields, Some(actual_model))?))
}

pub(crate) fn build_audio_transcription_form(
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<Form> {
    build_form(fields, Some(actual_model))
}

pub(crate) fn build_audio_translation_form(
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<Form> {
    build_form(fields, Some(actual_model))
}

pub(crate) fn default_transcription_content_type(response_format: Option<&str>) -> &'static str {
    match response_format.unwrap_or("json") {
        "text" | "srt" | "vtt" => "text/plain; charset=utf-8",
        _ => "application/json",
    }
}

pub(crate) fn binary_response(body: Bytes, content_type: &str) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .body(body.into())
        .expect("binary response");
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response
}

pub(crate) fn join_upstream_url(base_url: &str, path: &str) -> anyhow::Result<Url> {
    let base_url = base_url.trim_end_matches('/');
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    Url::parse(&format!("{base_url}{path}")).context("failed to parse upstream url")
}

fn build_json_request<T: serde::Serialize>(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    path: &str,
    req: &T,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    let payload = json_payload_with_model(req, actual_model)?;
    Ok(client
        .post(join_upstream_url(base_url, path)?)
        .bearer_auth(api_key)
        .json(&payload))
}

fn json_payload_with_model<T: serde::Serialize>(
    req: &T,
    actual_model: &str,
) -> anyhow::Result<serde_json::Value> {
    let mut payload = serde_json::to_value(req).context("failed to serialize request")?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("request payload must be a JSON object"))?;
    object.insert(
        "model".into(),
        serde_json::Value::String(actual_model.to_string()),
    );
    Ok(payload)
}

fn build_form(
    fields: &[BufferedMultipartField],
    actual_model: Option<&str>,
) -> anyhow::Result<Form> {
    let mut form = Form::new();
    let mut wrote_model = false;

    for field in fields {
        match (&field.filename, &field.content_type) {
            (Some(filename), content_type) => {
                let mut part =
                    Part::bytes(field.bytes.clone().to_vec()).file_name(filename.clone());
                if let Some(content_type) = content_type {
                    part = part.mime_str(content_type).with_context(|| {
                        format!("invalid multipart content type: {content_type}")
                    })?;
                }
                form = form.part(field.name.clone(), part);
            }
            (None, _) => {
                let value = String::from_utf8(field.bytes.to_vec()).with_context(|| {
                    format!("multipart field '{}' is not valid UTF-8", field.name)
                })?;
                if field.name == "model" {
                    wrote_model = true;
                    form = form.text(
                        field.name.clone(),
                        actual_model.unwrap_or(&value).to_string(),
                    );
                } else {
                    form = form.text(field.name.clone(), value);
                }
            }
        }
    }

    if !wrote_model && let Some(actual_model) = actual_model {
        form = form.text("model".to_string(), actual_model.to_string());
    }

    Ok(form)
}

fn required_text_field(fields: &[BufferedMultipartField], name: &str) -> anyhow::Result<String> {
    optional_text_field(fields, name).ok_or_else(|| anyhow!("missing field: {name}"))
}

fn optional_text_field(fields: &[BufferedMultipartField], name: &str) -> Option<String> {
    fields.iter().find_map(|field| {
        (field.name == name && field.filename.is_none())
            .then(|| String::from_utf8(field.bytes.to_vec()).ok())
            .flatten()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn estimate_text_tokens(value: &str) -> i32 {
    (((value.len() as f64) / 4.0).ceil() as i32).max(1)
}
