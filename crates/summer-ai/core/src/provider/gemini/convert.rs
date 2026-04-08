use anyhow::{Context, Result};
use bytes::Bytes;

use crate::convert::{joined_text_value, serialize_arguments};
use crate::types::chat::{ChatCompletionResponse, Choice};
use crate::types::common::{FinishReason, FunctionCall, Message, ToolCall, Usage};
use crate::types::embedding::{EmbeddingData, EmbeddingResponse};

use super::protocol::{
    GeminiCandidate, GeminiEmbedContentResponse, GeminiResponse, GeminiUsageMetadata,
};

pub(super) fn parse_embedding_response(
    body: Bytes,
    estimated_prompt_tokens: i32,
) -> Result<EmbeddingResponse> {
    let response: GeminiEmbedContentResponse =
        serde_json::from_slice(&body).context("failed to deserialize gemini embedding response")?;

    let embeddings = if !response.embeddings.is_empty() {
        response.embeddings
    } else if let Some(embedding) = response.embedding {
        vec![embedding]
    } else {
        Vec::new()
    };

    if embeddings.is_empty() {
        anyhow::bail!("gemini embedding response did not contain any embeddings");
    }

    Ok(EmbeddingResponse {
        object: "list".into(),
        data: embeddings
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| EmbeddingData {
                object: "embedding".into(),
                index: index as i32,
                embedding: serde_json::json!(embedding.values),
            })
            .collect(),
        usage: Usage {
            prompt_tokens: estimated_prompt_tokens.max(0),
            completion_tokens: 0,
            total_tokens: estimated_prompt_tokens.max(0),
            cached_tokens: 0,
            reasoning_tokens: 0,
        },
        extra: serde_json::Map::new(),
    })
}

pub(super) fn convert_response(
    response: GeminiResponse,
    model: &str,
) -> Result<ChatCompletionResponse> {
    let usage = response
        .usage_metadata
        .map(usage_from_gemini)
        .unwrap_or_default();
    let choices = response
        .candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| choice_from_gemini_candidate(candidate, index as i32))
        .collect::<Vec<_>>();

    if choices.is_empty() {
        anyhow::bail!("gemini response did not contain any candidates");
    }

    Ok(ChatCompletionResponse {
        id: format!("gemini-{}", super::unix_timestamp()),
        object: "chat.completion".into(),
        created: super::unix_timestamp(),
        model: model.to_string(),
        choices,
        usage,
    })
}

pub(super) fn usage_from_gemini(usage: GeminiUsageMetadata) -> Usage {
    let total_tokens = if usage.total_token_count > 0 {
        usage.total_token_count
    } else {
        usage.prompt_token_count + usage.candidates_token_count
    };

    Usage {
        prompt_tokens: usage.prompt_token_count,
        completion_tokens: usage.candidates_token_count,
        total_tokens,
        cached_tokens: 0,
        reasoning_tokens: 0,
    }
}

pub(super) fn map_gemini_finish_reason(
    finish_reason: Option<&str>,
    has_tool_calls: bool,
) -> Option<FinishReason> {
    if has_tool_calls {
        return Some(FinishReason::ToolCalls);
    }

    match finish_reason {
        Some("MAX_TOKENS" | "MAX_OUTPUT_TOKENS") => Some(FinishReason::Length),
        Some("SAFETY" | "RECITATION" | "BLOCKLIST") => Some(FinishReason::ContentFilter),
        Some(_) | None => Some(FinishReason::Stop),
    }
}

pub(super) fn map_gemini_stream_finish_reason(
    finish_reason: Option<&str>,
    has_tool_calls: bool,
) -> Option<FinishReason> {
    finish_reason.and_then(|reason| map_gemini_finish_reason(Some(reason), has_tool_calls))
}

fn choice_from_gemini_candidate(candidate: GeminiCandidate, index: i32) -> Choice {
    let mut texts = Vec::new();
    let mut tool_calls = Vec::new();

    if let Some(content) = candidate.content {
        for part in content.parts {
            if let Some(text) = part.text
                && !text.is_empty()
            {
                texts.push(text);
            }

            if let Some(function_call) = part.function_call {
                let tool_call_index = tool_calls.len() as i32;
                tool_calls.push(ToolCall {
                    id: format!("call_{index}_{tool_call_index}"),
                    r#type: "function".into(),
                    function: FunctionCall {
                        name: function_call.name,
                        arguments: serialize_arguments(function_call.args),
                    },
                });
            }
        }
    }

    let finish_reason =
        map_gemini_finish_reason(candidate.finish_reason.as_deref(), !tool_calls.is_empty());

    Choice {
        index,
        message: Message {
            role: "assistant".into(),
            content: joined_text_value(texts),
            name: None,
            tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            tool_call_id: None,
        },
        finish_reason,
    }
}
