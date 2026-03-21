use serde::Serialize;

/// GET /v1/models 响应
#[derive(Debug, Clone, Serialize)]
pub struct ModelListResponse {
    pub object: String,
    pub data: Vec<ModelObject>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelObject {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}
