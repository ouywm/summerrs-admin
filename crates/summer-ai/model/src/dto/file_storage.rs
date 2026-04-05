use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::file;
use crate::entity::vector_store;

// ─── File ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileDto {
    pub filename: String,
    #[serde(default = "default_purpose")]
    pub purpose: String,
    pub content_type: String,
    pub size_bytes: i64,
    #[serde(default)]
    pub project_id: i64,
    #[serde(default)]
    pub storage_backend: String,
    #[serde(default)]
    pub storage_path: String,
}

fn default_purpose() -> String {
    "assistants".into()
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryFileDto {
    pub project_id: Option<i64>,
    pub purpose: Option<String>,
    pub status: Option<file::FileStatus>,
    pub filename: Option<String>,
}

impl From<QueryFileDto> for sea_orm::Condition {
    fn from(dto: QueryFileDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.project_id {
            cond = cond.add(file::Column::ProjectId.eq(v));
        }
        if let Some(v) = dto.purpose {
            cond = cond.add(file::Column::Purpose.eq(v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(file::Column::Status.eq(v));
        }
        if let Some(v) = dto.filename {
            cond = cond.add(file::Column::Filename.contains(&v));
        }
        cond
    }
}

// ─── VectorStore ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateVectorStoreDto {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub project_id: i64,
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_dimensions")]
    pub embedding_dimensions: i32,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub expires_after: serde_json::Value,
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".into()
}
fn default_dimensions() -> i32 {
    1536
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVectorStoreDto {
    pub name: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub expires_after: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryVectorStoreDto {
    pub project_id: Option<i64>,
    pub name: Option<String>,
    pub status: Option<i16>,
}

impl From<QueryVectorStoreDto> for sea_orm::Condition {
    fn from(dto: QueryVectorStoreDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all().add(vector_store::Column::DeletedAt.is_null());
        if let Some(v) = dto.project_id {
            cond = cond.add(vector_store::Column::ProjectId.eq(v));
        }
        if let Some(v) = dto.name {
            cond = cond.add(vector_store::Column::Name.contains(&v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(vector_store::Column::Status.eq(v));
        }
        cond
    }
}
