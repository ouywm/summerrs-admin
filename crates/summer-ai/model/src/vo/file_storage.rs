use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::file;
use crate::entity::vector_store;
use crate::entity::vector_store_file;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileVo {
    pub id: i64,
    pub filename: String,
    pub purpose: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_backend: String,
    pub status: file::FileStatus,
    pub project_id: i64,
    pub expires_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
}

impl FileVo {
    pub fn from_model(m: file::Model) -> Self {
        Self {
            id: m.id,
            filename: m.filename,
            purpose: m.purpose,
            content_type: m.content_type,
            size_bytes: m.size_bytes,
            storage_backend: m.storage_backend,
            status: m.status,
            project_id: m.project_id,
            expires_at: m.expires_at,
            create_time: m.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VectorStoreVo {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub embedding_model: String,
    pub embedding_dimensions: i32,
    pub storage_backend: String,
    pub status: i16,
    pub usage_bytes: i64,
    pub file_counts: serde_json::Value,
    pub metadata: serde_json::Value,
    pub project_id: i64,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl VectorStoreVo {
    pub fn from_model(m: vector_store::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            description: m.description,
            embedding_model: m.embedding_model,
            embedding_dimensions: m.embedding_dimensions,
            storage_backend: m.storage_backend,
            status: m.status,
            usage_bytes: m.usage_bytes,
            file_counts: m.file_counts,
            metadata: m.metadata,
            project_id: m.project_id,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VectorStoreFileVo {
    pub id: i64,
    pub vector_store_id: i64,
    pub file_id: i64,
    pub status: i16,
    pub usage_bytes: i64,
    pub last_error: serde_json::Value,
    pub chunking_strategy: serde_json::Value,
    pub create_time: DateTime<FixedOffset>,
}

impl VectorStoreFileVo {
    pub fn from_model(m: vector_store_file::Model) -> Self {
        Self {
            id: m.id,
            vector_store_id: m.vector_store_id,
            file_id: m.file_id,
            status: m.status,
            usage_bytes: m.usage_bytes,
            last_error: m.last_error,
            chunking_strategy: m.chunking_strategy,
            create_time: m.create_time,
        }
    }
}
