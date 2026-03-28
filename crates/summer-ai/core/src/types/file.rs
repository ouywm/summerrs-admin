use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileObject {
    pub id: String,
    pub object: String,
    pub bytes: i64,
    pub created_at: i64,
    pub filename: String,
    pub purpose: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileListResponse {
    pub object: String,
    pub data: Vec<FileObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    pub has_more: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileDeleteResponse {
    pub id: String,
    pub object: String,
    pub deleted: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn file_list_response_deserializes() {
        let response: FileListResponse = serde_json::from_value(serde_json::json!({
            "object": "list",
            "data": [{
                "id": "file-123",
                "object": "file",
                "bytes": 5,
                "created_at": 1700000000,
                "filename": "notes.txt",
                "purpose": "assistants"
            }],
            "has_more": false
        }))
        .unwrap();

        assert_eq!(response.data.len(), 1);
        assert!(!response.has_more);
    }
}
