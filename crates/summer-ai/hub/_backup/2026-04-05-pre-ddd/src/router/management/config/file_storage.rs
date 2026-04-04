use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::file_storage::FileStorageService;
use summer_ai_model::dto::file_storage::{
    CreateFileDto, CreateVectorStoreDto, QueryFileDto, QueryVectorStoreDto, UpdateVectorStoreDto,
};
use summer_ai_model::vo::file_storage::{FileVo, VectorStoreFileVo, VectorStoreVo};

// ─── File ───

#[get_api("/ai/file")]
pub async fn list_files(
    Component(svc): Component<FileStorageService>,
    Query(query): Query<QueryFileDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<FileVo>>> {
    Ok(Json(svc.list_files(query, pagination).await?))
}

#[get_api("/ai/file/{id}")]
pub async fn get_file(
    Component(svc): Component<FileStorageService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<FileVo>> {
    Ok(Json(svc.get_file(id).await?))
}

#[post_api("/ai/file")]
pub async fn create_file(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<FileStorageService>,
    ValidatedJson(dto): ValidatedJson<CreateFileDto>,
) -> ApiResult<Json<FileVo>> {
    Ok(Json(svc.create_file(dto, &profile.nick_name).await?))
}

#[delete_api("/ai/file/{id}")]
pub async fn delete_file(
    Component(svc): Component<FileStorageService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_file(id).await
}

// ─── VectorStore ───

#[get_api("/ai/vector-store")]
pub async fn list_vector_stores(
    Component(svc): Component<FileStorageService>,
    Query(query): Query<QueryVectorStoreDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<VectorStoreVo>>> {
    Ok(Json(svc.list_vector_stores(query, pagination).await?))
}

#[get_api("/ai/vector-store/{id}")]
pub async fn get_vector_store(
    Component(svc): Component<FileStorageService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<VectorStoreVo>> {
    Ok(Json(svc.get_vector_store(id).await?))
}

#[post_api("/ai/vector-store")]
pub async fn create_vector_store(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<FileStorageService>,
    ValidatedJson(dto): ValidatedJson<CreateVectorStoreDto>,
) -> ApiResult<Json<VectorStoreVo>> {
    Ok(Json(
        svc.create_vector_store(dto, &profile.nick_name).await?,
    ))
}

#[put_api("/ai/vector-store/{id}")]
pub async fn update_vector_store(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<FileStorageService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateVectorStoreDto>,
) -> ApiResult<Json<VectorStoreVo>> {
    Ok(Json(
        svc.update_vector_store(id, dto, &profile.nick_name).await?,
    ))
}

#[delete_api("/ai/vector-store/{id}")]
pub async fn delete_vector_store(
    Component(svc): Component<FileStorageService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_vector_store(id).await
}

// ─── VectorStoreFile ───

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddFileBody {
    pub file_id: i64,
}

#[get_api("/ai/vector-store/{store_id}/file")]
pub async fn list_vector_store_files(
    Component(svc): Component<FileStorageService>,
    Path(store_id): Path<i64>,
    pagination: Pagination,
) -> ApiResult<Json<Page<VectorStoreFileVo>>> {
    Ok(Json(
        svc.list_vector_store_files(store_id, pagination).await?,
    ))
}

#[post_api("/ai/vector-store/{store_id}/file")]
pub async fn add_file_to_vector_store(
    Component(svc): Component<FileStorageService>,
    Path(store_id): Path<i64>,
    summer_common::response::Json(body): summer_common::response::Json<AddFileBody>,
) -> ApiResult<Json<VectorStoreFileVo>> {
    Ok(Json(
        svc.add_file_to_vector_store(store_id, body.file_id).await?,
    ))
}

#[delete_api("/ai/vector-store-file/{id}")]
pub async fn remove_file_from_vector_store(
    Component(svc): Component<FileStorageService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.remove_file_from_vector_store(id).await
}
