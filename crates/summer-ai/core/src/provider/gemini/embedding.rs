use anyhow::{Context, Result};

use crate::convert::message::merge_extra_body_fields;
use crate::types::embedding::EmbeddingRequest;

use super::protocol::{
    GeminiBatchEmbedContentsRequest, GeminiEmbedContent, GeminiEmbedContentRequest, GeminiTextPart,
};

pub(super) fn build_gemini_embedding_body(
    req: &EmbeddingRequest,
    actual_model: &str,
) -> Result<serde_json::Value> {
    let inputs = extract_embedding_inputs(&req.input);
    let task_type = extract_embedding_extra_str(&req.extra, &["taskType", "task_type"]);
    let title = extract_embedding_extra_str(&req.extra, &["title"]);

    let mut body = if inputs.len() <= 1 {
        serde_json::to_value(GeminiEmbedContentRequest {
            model: format!("models/{actual_model}"),
            content: GeminiEmbedContent {
                parts: vec![GeminiTextPart {
                    text: inputs.into_iter().next().unwrap_or_default(),
                }],
            },
            task_type,
            title,
            output_dimensionality: req.dimensions,
        })
        .context("failed to serialize gemini embedding request")?
    } else {
        serde_json::to_value(GeminiBatchEmbedContentsRequest {
            requests: inputs
                .into_iter()
                .map(|text| GeminiEmbedContentRequest {
                    model: format!("models/{actual_model}"),
                    content: GeminiEmbedContent {
                        parts: vec![GeminiTextPart { text }],
                    },
                    task_type: task_type.clone(),
                    title: title.clone(),
                    output_dimensionality: req.dimensions,
                })
                .collect(),
        })
        .context("failed to serialize gemini batch embedding request")?
    };
    merge_extra_body_fields(&mut body, &req.extra);
    Ok(body)
}

pub(super) fn build_gemini_embedding_url(base_url: &str, model: &str, batch: bool) -> String {
    let action = if batch {
        "batchEmbedContents"
    } else {
        "embedContent"
    };

    format!("{}/models/{model}:{action}", gemini_version_base(base_url))
}

pub(super) fn is_batch_embedding_input(input: &serde_json::Value) -> bool {
    matches!(input, serde_json::Value::Array(items) if items.iter().all(serde_json::Value::is_string) && items.len() > 1)
}

pub(super) fn gemini_version_base(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1beta") || base.ends_with("/v1") {
        base.to_string()
    } else {
        format!("{base}/v1beta")
    }
}

fn extract_embedding_inputs(input: &serde_json::Value) -> Vec<String> {
    match input {
        serde_json::Value::String(text) => vec![text.clone()],
        serde_json::Value::Array(items)
            if items.iter().all(serde_json::Value::is_string) && !items.is_empty() =>
        {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        }
        other => vec![embedding_input_to_text(other)],
    }
}

fn embedding_input_to_text(input: &serde_json::Value) -> String {
    match input {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn extract_embedding_extra_str(
    extra: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        extra
            .get(*key)
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .filter(|value| !value.is_empty())
    })
}
