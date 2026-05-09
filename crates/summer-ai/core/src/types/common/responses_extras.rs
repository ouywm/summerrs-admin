use serde::{Deserialize, Serialize};
use serde_json::Value;

/// OpenAI Responses API 独有、但暂不适合直接塞进通用字段的 canonical 扩展。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesExtras {
    /// 上一轮 Responses 对话的 response id，用于多轮续接。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    /// Responses reasoning.summary。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<String>,
    /// Responses 顶层 instructions。当前 ingress 仍会同时展开成 system message。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// 无法投影到 canonical 通用 message/tool 结构的原生 Responses input items。
    ///
    /// 用于保留 `reasoning` / `computer_call` / `web_search_call` 等 provider-native item，
    /// 供后续转回 Responses wire 时恢复。当前仅保证“保留不丢”，不保证严格原位置重建。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_input_items: Vec<Value>,
}

#[cfg(test)]
mod tests {
    use super::ResponsesExtras;

    #[test]
    fn empty_extras_serializes_to_empty_object() {
        let value = serde_json::to_value(ResponsesExtras::default()).unwrap();
        assert_eq!(value, serde_json::json!({}));
    }

    #[test]
    fn roundtrip_preserves_all_fields() {
        let extras = ResponsesExtras {
            previous_response_id: Some("resp_123".into()),
            reasoning_summary: Some("concise".into()),
            instructions: Some("be terse".into()),
            native_input_items: vec![serde_json::json!({"type":"reasoning","id":"rs_1"})],
        };
        let value = serde_json::to_value(&extras).unwrap();
        assert_eq!(value["previous_response_id"], "resp_123");
        assert_eq!(value["reasoning_summary"], "concise");
        assert_eq!(value["instructions"], "be terse");
        assert_eq!(value["native_input_items"][0]["type"], "reasoning");

        let back: ResponsesExtras = serde_json::from_value(value).unwrap();
        assert_eq!(back, extras);
    }
}
