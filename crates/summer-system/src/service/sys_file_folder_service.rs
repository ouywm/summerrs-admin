//! 文件夹服务（文件中心）

use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_system_model::dto::sys_file_folder::{CreateFileFolderDto, UpdateFileFolderDto};
use summer_system_model::entity::{sys_file, sys_file_folder};
use summer_system_model::vo::sys_file_folder::{FileFolderTreeVo, FileFolderVo};

#[derive(Clone, Service)]
pub struct SysFileFolderService {
    #[inject(component)]
    db: DbConn,
}

#[derive(Debug, FromQueryResult)]
struct FolderCountRow {
    pub folder_id: Option<i64>,
    pub file_count: i64,
}

impl SysFileFolderService {
    /// 文件夹树查询
    pub async fn tree(&self) -> ApiResult<Vec<FileFolderTreeVo>> {
        let folders = sys_file_folder::Entity::find()
            .order_by_asc(sys_file_folder::Column::ParentId)
            .order_by_asc(sys_file_folder::Column::Sort)
            .order_by_asc(sys_file_folder::Column::Id)
            .all(&self.db)
            .await
            .context("查询文件夹列表失败")?;

        let counts: Vec<FolderCountRow> = sys_file::Entity::find()
            .select_only()
            .column(sys_file::Column::FolderId)
            .column_as(sys_file::Column::Id.count(), "file_count")
            .filter(sys_file::Column::DeletedAt.is_null())
            .filter(sys_file::Column::FolderId.is_not_null())
            .group_by(sys_file::Column::FolderId)
            .into_model::<FolderCountRow>()
            .all(&self.db)
            .await
            .context("统计文件夹文件数量失败")?;

        let mut count_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        for row in counts {
            if let Some(folder_id) = row.folder_id {
                count_map.insert(folder_id, row.file_count);
            }
        }

        let mut by_parent: std::collections::HashMap<i64, Vec<FileFolderTreeVo>> =
            std::collections::HashMap::new();

        for f in folders {
            let node = FileFolderTreeVo {
                id: f.id,
                parent_id: f.parent_id,
                name: f.name,
                slug: f.slug,
                visibility: f.visibility,
                sort: f.sort,
                file_count: count_map.get(&f.id).copied().unwrap_or(0),
                children: Vec::new(),
            };
            by_parent.entry(node.parent_id).or_default().push(node);
        }

        fn build_tree(
            by_parent: &mut std::collections::HashMap<i64, Vec<FileFolderTreeVo>>,
            parent_id: i64,
        ) -> Vec<FileFolderTreeVo> {
            let mut children = by_parent.remove(&parent_id).unwrap_or_default();
            for child in &mut children {
                // `file_count` should reflect the whole subtree (parent + descendants),
                // so the left folder tree can show counts even when files are placed in subfolders.
                let grand_children = build_tree(by_parent, child.id);
                let descendants_count: i64 = grand_children.iter().map(|c| c.file_count).sum();
                child.file_count += descendants_count;
                child.children = grand_children;
            }
            children
        }

        Ok(build_tree(&mut by_parent, 0))
    }

    pub async fn get_by_id(&self, id: i64) -> ApiResult<FileFolderVo> {
        let model = self.find_model_by_id(id).await?;

        // 动态计算 file_count（避免依赖冗余字段一致性）
        let file_count = sys_file::Entity::find()
            .filter(sys_file::Column::DeletedAt.is_null())
            .filter(sys_file::Column::FolderId.eq(id))
            .count(&self.db)
            .await
            .context("统计文件夹文件数量失败")? as i64;

        Ok(FileFolderVo::from_model(model, file_count))
    }

    pub async fn create(&self, dto: CreateFileFolderDto) -> ApiResult<FileFolderVo> {
        // 同级 slug 唯一
        let parent_id = dto.parent_id.unwrap_or(0);
        self.ensure_slug_unique(parent_id, &dto.slug, None).await?;

        let model = sys_file_folder::ActiveModel::from(dto)
            .insert(&self.db)
            .await
            .context("创建文件夹失败")?;
        Ok(FileFolderVo::from_model(model, 0))
    }

    pub async fn update(&self, id: i64, dto: UpdateFileFolderDto) -> ApiResult<FileFolderVo> {
        let model = self.find_model_by_id(id).await?;
        let parent_id = model.parent_id;
        let mut active: sys_file_folder::ActiveModel = model.into();

        if let Some(ref slug) = dto.slug {
            self.ensure_slug_unique(parent_id, slug, Some(id)).await?;
        }

        dto.apply_to(&mut active);
        let model = active.update(&self.db).await.context("更新文件夹失败")?;

        // 动态计算 file_count（避免依赖冗余字段一致性）
        let file_count = sys_file::Entity::find()
            .filter(sys_file::Column::DeletedAt.is_null())
            .filter(sys_file::Column::FolderId.eq(id))
            .count(&self.db)
            .await
            .context("统计文件夹文件数量失败")? as i64;

        Ok(FileFolderVo::from_model(model, file_count))
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let _ = self.find_model_by_id(id).await?;

        let child_count = sys_file_folder::Entity::find()
            .filter(sys_file_folder::Column::ParentId.eq(id))
            .count(&self.db)
            .await
            .context("查询子文件夹数量失败")?;
        if child_count > 0 {
            return Err(ApiErrors::BadRequest(
                "文件夹下仍存在子文件夹，无法删除".to_string(),
            ));
        }

        let file_count = sys_file::Entity::find()
            .filter(sys_file::Column::DeletedAt.is_null())
            .filter(sys_file::Column::FolderId.eq(id))
            .count(&self.db)
            .await
            .context("查询文件夹内文件数量失败")?;
        if file_count > 0 {
            return Err(ApiErrors::BadRequest(
                "文件夹下仍存在文件，无法删除".to_string(),
            ));
        }

        let result = sys_file_folder::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除文件夹失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("文件夹不存在".to_string()));
        }

        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<sys_file_folder::Model> {
        sys_file_folder::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询文件夹失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件夹不存在".to_string()))
    }

    async fn ensure_slug_unique(
        &self,
        parent_id: i64,
        slug: &str,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut query = sys_file_folder::Entity::find()
            .filter(sys_file_folder::Column::ParentId.eq(parent_id))
            .filter(sys_file_folder::Column::Slug.eq(slug));

        if let Some(exclude_id) = exclude_id {
            query = query.filter(sys_file_folder::Column::Id.ne(exclude_id));
        }

        let exists = query
            .one(&self.db)
            .await
            .context("查询文件夹slug唯一性失败")?
            .is_some();

        if exists {
            return Err(ApiErrors::BadRequest("同级slug已存在".to_string()));
        }

        Ok(())
    }
}
