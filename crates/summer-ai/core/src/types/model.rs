use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// GET /v1/models 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelListResponse {
    pub object: String,
    pub data: Vec<ModelObject>,
}

/// 模型对象
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelObject {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}

impl ModelListResponse {
    /// 从模型名称列表构建响应
    pub fn from_model_names(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            object: "list".into(),
            data: names
                .into_iter()
                .map(|name| ModelObject {
                    id: name.into(),
                    object: "model".into(),
                    created: 0,
                    owned_by: "system".into(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_list_response_serialize() {
        let resp = ModelListResponse {
            object: "list".into(),
            data: vec![ModelObject {
                id: "gpt-4".into(),
                object: "model".into(),
                created: 1700000000,
                owned_by: "openai".into(),
            }],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["object"], "list");
        assert_eq!(json["data"][0]["id"], "gpt-4");
        assert_eq!(json["data"][0]["owned_by"], "openai");
    }

    #[test]
    fn model_list_response_deserialize() {
        let json = serde_json::json!({
            "object": "list",
            "data": [
                {
                    "id": "gpt-4",
                    "object": "model",
                    "created": 1700000000,
                    "owned_by": "openai"
                },
                {
                    "id": "gpt-3.5-turbo",
                    "object": "model",
                    "created": 1600000000,
                    "owned_by": "openai"
                }
            ]
        });
        let resp: ModelListResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].id, "gpt-4");
        assert_eq!(resp.data[1].id, "gpt-3.5-turbo");
    }

    #[test]
    fn from_model_names() {
        let resp = ModelListResponse::from_model_names(["gpt-4", "claude-3"]);
        assert_eq!(resp.object, "list");
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].id, "gpt-4");
        assert_eq!(resp.data[1].id, "claude-3");
        assert_eq!(resp.data[0].object, "model");
        assert_eq!(resp.data[0].owned_by, "system");
    }

    #[test]
    fn from_model_names_empty() {
        let resp = ModelListResponse::from_model_names(Vec::<String>::new());
        assert_eq!(resp.data.len(), 0);
    }

    #[test]
    fn model_object_round_trip() {
        let model = ModelObject {
            id: "deepseek-chat".into(),
            object: "model".into(),
            created: 1700000000,
            owned_by: "deepseek".into(),
        };
        let json = serde_json::to_string(&model).unwrap();
        let parsed: ModelObject = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "deepseek-chat");
        assert_eq!(parsed.owned_by, "deepseek");
    }
}
