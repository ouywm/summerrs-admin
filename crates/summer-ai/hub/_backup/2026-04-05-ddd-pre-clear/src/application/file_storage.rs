use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::file_storage::{
    CreateFileDto, CreateVectorStoreDto, QueryFileDto, QueryVectorStoreDto, UpdateVectorStoreDto,
};
use summer_ai_model::entity::file;
use summer_ai_model::entity::vector_store;
use summer_ai_model::entity::vector_store_file;
use summer_ai_model::vo::file_storage::{FileVo, VectorStoreFileVo, VectorStoreVo};

#[derive(Clone, Service)]
pub struct FileStorageService {
    #[inject(component)]
    db: DbConn,
}

impl FileStorageService {
    // ─── File ───

    pub async fn list_files(
        &self,
        query: QueryFileDto,
        pagination: Pagination,
    ) -> ApiResult<Page<FileVo>> {
        let page = file::Entity::find()
            .filter(query)
            .order_by_desc(file::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询文件列表失败")?;
        Ok(page.map(FileVo::from_model))
    }

    pub async fn get_file(&self, id: i64) -> ApiResult<FileVo> {
        let model = file::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询文件失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;
        Ok(FileVo::from_model(model))
    }

    pub async fn create_file(&self, dto: CreateFileDto, operator: &str) -> ApiResult<FileVo> {
        let now = chrono::Utc::now().fixed_offset();
        let active = file::ActiveModel {
            owner_type: Set("project".into()),
            owner_id: Set(dto.project_id),
            project_id: Set(dto.project_id),
            session_id: Set(0),
            trace_id: Set(0),
            request_id: Set(String::new()),
            filename: Set(dto.filename),
            purpose: Set(dto.purpose),
            content_type: Set(dto.content_type),
            size_bytes: Set(dto.size_bytes),
            content_hash: Set(String::new()),
            storage_backend: Set(if dto.storage_backend.is_empty() {
                "database".into()
            } else {
                dto.storage_backend
            }),
            storage_path: Set(dto.storage_path),
            provider_file_id: Set(String::new()),
            status: Set(1),
            status_detail: Set(String::new()),
            metadata: Set(serde_json::json!({})),
            expires_at: Set(None),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        let model = active
            .insert(&self.db)
            .await
            .context("创建文件失败")
            .map_err(ApiErrors::Internal)?;
        Ok(FileVo::from_model(model))
    }

    pub async fn delete_file(&self, id: i64) -> ApiResult<()> {
        file::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除文件失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── VectorStore ───

    pub async fn list_vector_stores(
        &self,
        query: QueryVectorStoreDto,
        pagination: Pagination,
    ) -> ApiResult<Page<VectorStoreVo>> {
        let page = vector_store::Entity::find()
            .filter(query)
            .order_by_desc(vector_store::Column::UpdateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询向量库列表失败")?;
        Ok(page.map(VectorStoreVo::from_model))
    }

    pub async fn get_vector_store(&self, id: i64) -> ApiResult<VectorStoreVo> {
        let model = vector_store::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询向量库失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("向量库不存在".to_string()))?;
        Ok(VectorStoreVo::from_model(model))
    }

    pub async fn create_vector_store(
        &self,
        dto: CreateVectorStoreDto,
        operator: &str,
    ) -> ApiResult<VectorStoreVo> {
        let now = chrono::Utc::now().fixed_offset();
        let active = vector_store::ActiveModel {
            owner_type: Set("project".into()),
            owner_id: Set(dto.project_id),
            project_id: Set(dto.project_id),
            name: Set(dto.name),
            description: Set(dto.description),
            embedding_model: Set(dto.embedding_model),
            embedding_dimensions: Set(dto.embedding_dimensions),
            storage_backend: Set("pgvector".into()),
            provider_vector_store_id: Set(String::new()),
            status: Set(1),
            usage_bytes: Set(0),
            file_counts: Set(
                serde_json::json!({"cancelled":0,"completed":0,"failed":0,"in_progress":0,"total":0}),
            ),
            metadata: Set(dto.metadata),
            expires_after: Set(dto.expires_after),
            expires_at: Set(None),
            last_active_at: Set(None),
            deleted_at: Set(None),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        let model = active
            .insert(&self.db)
            .await
            .context("创建向量库失败")
            .map_err(ApiErrors::Internal)?;
        Ok(VectorStoreVo::from_model(model))
    }

    pub async fn update_vector_store(
        &self,
        id: i64,
        dto: UpdateVectorStoreDto,
        operator: &str,
    ) -> ApiResult<VectorStoreVo> {
        let model = vector_store::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询向量库失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("向量库不存在".to_string()))?;
        let mut active: vector_store::ActiveModel = model.into();
        if let Some(v) = dto.name {
            active.name = Set(v);
        }
        if let Some(v) = dto.description {
            active.description = Set(v);
        }
        if let Some(v) = dto.metadata {
            active.metadata = Set(v);
        }
        if let Some(v) = dto.expires_after {
            active.expires_after = Set(v);
        }
        active.update_by = Set(operator.to_string());
        let updated = active
            .update(&self.db)
            .await
            .context("更新向量库失败")
            .map_err(ApiErrors::Internal)?;
        Ok(VectorStoreVo::from_model(updated))
    }

    pub async fn delete_vector_store(&self, id: i64) -> ApiResult<()> {
        let model = vector_store::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询向量库失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("向量库不存在".to_string()))?;
        let mut active: vector_store::ActiveModel = model.into();
        active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active
            .update(&self.db)
            .await
            .context("软删除向量库失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── VectorStoreFile ───

    pub async fn list_vector_store_files(
        &self,
        store_id: i64,
        pagination: Pagination,
    ) -> ApiResult<Page<VectorStoreFileVo>> {
        let page = vector_store_file::Entity::find()
            .filter(vector_store_file::Column::VectorStoreId.eq(store_id))
            .filter(vector_store_file::Column::DeletedAt.is_null())
            .order_by_desc(vector_store_file::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询向量库文件失败")?;
        Ok(page.map(VectorStoreFileVo::from_model))
    }

    pub async fn add_file_to_vector_store(
        &self,
        store_id: i64,
        file_id: i64,
    ) -> ApiResult<VectorStoreFileVo> {
        let now = chrono::Utc::now().fixed_offset();
        let active = vector_store_file::ActiveModel {
            vector_store_id: Set(store_id),
            file_id: Set(file_id),
            status: Set(1),
            usage_bytes: Set(0),
            last_error: Set(serde_json::json!({})),
            chunking_strategy: Set(serde_json::json!({})),
            attributes: Set(serde_json::json!({})),
            deleted_at: Set(None),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        let model = active
            .insert(&self.db)
            .await
            .context("添加文件到向量库失败")
            .map_err(ApiErrors::Internal)?;
        Ok(VectorStoreFileVo::from_model(model))
    }

    pub async fn remove_file_from_vector_store(&self, id: i64) -> ApiResult<()> {
        let model = vector_store_file::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询向量库文件失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("向量库文件不存在".to_string()))?;
        let mut active: vector_store_file::ActiveModel = model.into();
        active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active
            .update(&self.db)
            .await
            .context("移除向量库文件失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }
}
