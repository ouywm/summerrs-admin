use serde::{Deserialize, Serialize};

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
        };
        let value = serde_json::to_value(&extras).unwrap();
        assert_eq!(value["previous_response_id"], "resp_123");
        assert_eq!(value["reasoning_summary"], "concise");
        assert_eq!(value["instructions"], "be terse");

        let back: ResponsesExtras = serde_json::from_value(value).unwrap();
        assert_eq!(back, extras);
    }
}
