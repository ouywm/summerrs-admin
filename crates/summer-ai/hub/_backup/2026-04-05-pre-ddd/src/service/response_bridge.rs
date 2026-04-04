use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_common::error::ApiResult;

use summer_ai_core::types::responses::ResponsesResponse;

use crate::service::runtime_cache::RuntimeCacheService;
use crate::service::token::TokenInfo;

const RESPONSE_BRIDGE_TTL_SECONDS: u64 = 24 * 60 * 60;

#[derive(Clone, Service)]
pub struct ResponseBridgeService {
    #[inject(component)]
    cache: RuntimeCacheService,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResponseBridgeRecord {
    response: ResponsesResponse,
    input_items: serde_json::Value,
    #[serde(default)]
    upstream_request_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ResponseBridgeSnapshot<T> {
    pub(crate) payload: T,
    pub(crate) upstream_request_id: String,
}

impl ResponseBridgeService {
    pub fn new(cache: RuntimeCacheService) -> Self {
        Self { cache }
    }

    pub async fn store(
        &self,
        token_info: &TokenInfo,
        response: ResponsesResponse,
        input: &serde_json::Value,
        upstream_request_id: &str,
    ) -> ApiResult<()> {
        let record = ResponseBridgeRecord {
            response: response.clone(),
            input_items: build_input_items_payload(input),
            upstream_request_id: upstream_request_id.to_string(),
        };
        self.cache
            .set_json(
                &response_bridge_cache_key(token_info.token_id, &response.id),
                &record,
                RESPONSE_BRIDGE_TTL_SECONDS,
            )
            .await
    }

    pub(crate) async fn get_response(
        &self,
        token_info: &TokenInfo,
        response_id: &str,
    ) -> ApiResult<Option<ResponseBridgeSnapshot<ResponsesResponse>>> {
        Ok(self
            .cache
            .get_json::<ResponseBridgeRecord>(&response_bridge_cache_key(
                token_info.token_id,
                response_id,
            ))
            .await?
            .map(|record| ResponseBridgeSnapshot {
                payload: record.response,
                upstream_request_id: record.upstream_request_id,
            }))
    }

    pub(crate) async fn get_input_items(
        &self,
        token_info: &TokenInfo,
        response_id: &str,
    ) -> ApiResult<Option<ResponseBridgeSnapshot<serde_json::Value>>> {
        Ok(self
            .cache
            .get_json::<ResponseBridgeRecord>(&response_bridge_cache_key(
                token_info.token_id,
                response_id,
            ))
            .await?
            .map(|record| ResponseBridgeSnapshot {
                payload: record.input_items,
                upstream_request_id: record.upstream_request_id,
            }))
    }

    pub(crate) async fn cancel(
        &self,
        token_info: &TokenInfo,
        response_id: &str,
    ) -> ApiResult<Option<ResponseBridgeSnapshot<ResponsesResponse>>> {
        let cache_key = response_bridge_cache_key(token_info.token_id, response_id);
        let Some(mut record) = self
            .cache
            .get_json::<ResponseBridgeRecord>(&cache_key)
            .await?
        else {
            return Ok(None);
        };
        record.response.status = "cancelled".into();
        self.cache
            .set_json(&cache_key, &record, RESPONSE_BRIDGE_TTL_SECONDS)
            .await?;
        Ok(Some(ResponseBridgeSnapshot {
            payload: record.response,
            upstream_request_id: record.upstream_request_id,
        }))
    }
}

fn response_bridge_cache_key(token_id: i64, response_id: &str) -> String {
    format!("ai:response-bridge:{token_id}:{response_id}")
}

fn build_input_items_payload(input: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "object": "list",
        "data": input_items_data(input),
    })
}

fn input_items_data(input: &serde_json::Value) -> Vec<serde_json::Value> {
    match input {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(text) => vec![serde_json::json!({
            "type": "message",
            "role": "user",
            "content": text,
        })],
        serde_json::Value::Array(items) => items.iter().map(normalize_input_item).collect(),
        other => vec![normalize_input_item(other)],
    }
}

fn normalize_input_item(item: &serde_json::Value) -> serde_json::Value {
    if item.get("type").is_some() || item.get("role").is_some() {
        return item.clone();
    }

    serde_json::json!({
        "type": "message",
        "role": "user",
        "content": item,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn build_input_items_payload_wraps_string_input_as_user_message() {
        let payload = build_input_items_payload(&serde_json::json!("hello"));
        assert_eq!(payload["object"], "list");
        assert_eq!(payload["data"][0]["role"], "user");
        assert_eq!(payload["data"][0]["content"], "hello");
    }

    #[test]
    fn build_input_items_payload_keeps_structured_items() {
        let payload = build_input_items_payload(&serde_json::json!([
            {"type": "message", "role": "user", "content": "hello"}
        ]));
        assert_eq!(payload["data"][0]["type"], "message");
        assert_eq!(payload["data"][0]["content"], "hello");
    }

    #[test]
    fn response_bridge_record_defaults_upstream_request_id_for_legacy_cache() -> Result<()> {
        let record: ResponseBridgeRecord = serde_json::from_value(serde_json::json!({
            "response": {
                "id": "resp_legacy",
                "object": "response",
                "created_at": 1,
                "model": "gpt-5.4",
                "status": "completed"
            },
            "input_items": {
                "object": "list",
                "data": []
            }
        }))?;

        assert_eq!(record.upstream_request_id, "");

        Ok(())
    }
}
