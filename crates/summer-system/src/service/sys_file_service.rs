//! 系统文件管理服务（列表、详情、删除）

use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_system_model::dto::sys_file::{
    FileQueryDto, GeneratePublicLinkDto, MoveFileDto, UpdateFileDisplayNameDto,
    UpdateFileStatusDto, UpdateFileVisibilityDto,
};
use summer_system_model::entity::{sys_file, sys_file_folder, sys_user};
use summer_system_model::vo::sys_file::{
    FileCreatorSummaryVo, FileFolderSummaryVo, FileListSummaryVo, FilePageVo, FilePublicLinkVo,
    FileVo,
};

use summer_plugins::background_task::BackgroundTaskQueue;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct SysFileService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    s3: aws_sdk_s3::Client,
    #[inject(component)]
    task_queue: BackgroundTaskQueue,
}

#[derive(Debug, FromQueryResult)]
struct FolderEdgeRow {
    id: i64,
    parent_id: i64,
}

impl SysFileService {
    /// Expand a folder id into the whole subtree ids (including itself).
    ///
    /// This is used by file list filtering so selecting a parent folder in the left tree
    /// can list files in all descendant folders.
    async fn subtree_folder_ids(&self, root_id: i64) -> ApiResult<Vec<i64>> {
        let rows: Vec<FolderEdgeRow> = sys_file_folder::Entity::find()
            .select_only()
            .column(sys_file_folder::Column::Id)
            .column(sys_file_folder::Column::ParentId)
            .into_model::<FolderEdgeRow>()
            .all(&self.db)
            .await
            .context("查询文件夹层级关系失败")?;

        let mut by_parent: std::collections::HashMap<i64, Vec<i64>> =
            std::collections::HashMap::new();
        for row in rows {
            by_parent.entry(row.parent_id).or_default().push(row.id);
        }

        let mut visited: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut out: Vec<i64> = Vec::new();
        let mut stack: Vec<i64> = vec![root_id];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            out.push(id);
            if let Some(children) = by_parent.get(&id) {
                // Depth-first, order isn't important for SQL IN filter.
                stack.extend(children.iter().copied());
            }
        }
        Ok(out)
    }

    /// 文件列表（分页）
    pub async fn list_files(
        &self,
        query: FileQueryDto,
        pagination: Pagination,
    ) -> ApiResult<FilePageVo> {
        // `folderId` on the UI is a tree selection; it should match files in the whole subtree
        // (folder itself + descendants). So we take it out from the generic Condition builder
        // and handle it as an IN filter here.
        let FileQueryDto {
            file_no,
            original_name,
            display_name,
            extension,
            bucket,
            provider,
            kind,
            visibility,
            status,
            folder_id,
            creator_id,
        } = query;

        let mut cond = sea_orm::Condition::from(FileQueryDto {
            file_no,
            original_name,
            display_name,
            extension,
            bucket,
            provider,
            kind,
            visibility,
            status,
            folder_id: None,
            creator_id,
        });

        if let Some(folder_id) = folder_id {
            let ids = self.subtree_folder_ids(folder_id).await?;
            cond = cond.add(sys_file::Column::FolderId.is_in(ids));
        }

        let page = sys_file::Entity::find()
            .filter(cond.clone())
            .filter(sys_file::Column::DeletedAt.is_null())
            .order_by_desc(sys_file::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询文件列表失败")?;

        // summary：total/private/public（不受分页影响）
        let total = sys_file::Entity::find()
            .filter(cond.clone())
            .filter(sys_file::Column::DeletedAt.is_null())
            .count(&self.db)
            .await
            .context("统计文件总数失败")?;
        let private_count = sys_file::Entity::find()
            .filter(cond.clone())
            .filter(sys_file::Column::DeletedAt.is_null())
            .filter(sys_file::Column::Visibility.eq("PRIVATE"))
            .count(&self.db)
            .await
            .context("统计私有文件数量失败")?;
        let public_count = sys_file::Entity::find()
            .filter(cond.clone())
            .filter(sys_file::Column::DeletedAt.is_null())
            .filter(sys_file::Column::Visibility.eq("PUBLIC"))
            .count(&self.db)
            .await
            .context("统计公开文件数量失败")?;

        // 批量加载 folder / creator 摘要，避免 N+1
        let folder_ids: Vec<i64> = page.content.iter().filter_map(|m| m.folder_id).collect();
        let creator_ids: Vec<i64> = page.content.iter().filter_map(|m| m.creator_id).collect();

        let folders = if folder_ids.is_empty() {
            Vec::new()
        } else {
            sys_file_folder::Entity::find()
                .filter(sys_file_folder::Column::Id.is_in(folder_ids))
                .all(&self.db)
                .await
                .context("加载文件夹信息失败")?
        };
        let users = if creator_ids.is_empty() {
            Vec::new()
        } else {
            sys_user::Entity::find()
                .filter(sys_user::Column::Id.is_in(creator_ids))
                .all(&self.db)
                .await
                .context("加载创建人信息失败")?
        };

        let folder_map: std::collections::HashMap<i64, FileFolderSummaryVo> = folders
            .into_iter()
            .map(|f| {
                (
                    f.id,
                    FileFolderSummaryVo {
                        id: f.id,
                        parent_id: f.parent_id,
                        name: f.name,
                        slug: f.slug,
                        visibility: f.visibility,
                        sort: f.sort,
                    },
                )
            })
            .collect();
        let user_map: std::collections::HashMap<i64, FileCreatorSummaryVo> = users
            .into_iter()
            .map(|u| {
                (
                    u.id,
                    FileCreatorSummaryVo {
                        id: u.id,
                        user_name: u.user_name,
                        nick_name: u.nick_name,
                    },
                )
            })
            .collect();

        let current = page.page;
        let size = page.size;
        let total_page = page.total_elements;
        let models = page.content;

        let records: Vec<FileVo> = models
            .into_iter()
            .map(|m| {
                let folder = m.folder_id.and_then(|id| folder_map.get(&id).cloned());
                let creator = m.creator_id.and_then(|id| user_map.get(&id).cloned());
                let mut vo = FileVo::from_model(m);
                vo.folder = folder;
                vo.creator = creator;
                vo
            })
            .collect();

        Ok(FilePageVo {
            current,
            size,
            total: total_page,
            records,
            summary: FileListSummaryVo {
                total,
                private_count,
                public_count,
            },
        })
    }

    /// 文件详情
    pub async fn get_file(&self, file_id: i64) -> ApiResult<FileVo> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        if file.deleted_at.is_some() {
            return Err(ApiErrors::NotFound("文件不存在".to_string()));
        }

        let folder = if let Some(folder_id) = file.folder_id {
            sys_file_folder::Entity::find_by_id(folder_id)
                .one(&self.db)
                .await
                .context("加载文件夹信息失败")?
                .map(|f| FileFolderSummaryVo {
                    id: f.id,
                    parent_id: f.parent_id,
                    name: f.name,
                    slug: f.slug,
                    visibility: f.visibility,
                    sort: f.sort,
                })
        } else {
            None
        };
        let creator = if let Some(creator_id) = file.creator_id {
            sys_user::Entity::find_by_id(creator_id)
                .one(&self.db)
                .await
                .context("加载创建人信息失败")?
                .map(|u| FileCreatorSummaryVo {
                    id: u.id,
                    user_name: u.user_name,
                    nick_name: u.nick_name,
                })
        } else {
            None
        };

        let mut vo = FileVo::from_model(file);
        vo.folder = folder;
        vo.creator = creator;
        Ok(vo)
    }

    /// 生成公开分享链接
    pub async fn generate_public_link(
        &self,
        file_id: i64,
        dto: GeneratePublicLinkDto,
    ) -> ApiResult<FilePublicLinkVo> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        if file.deleted_at.is_some() {
            return Err(ApiErrors::NotFound("文件不存在".to_string()));
        }

        let token = uuid::Uuid::new_v4().simple().to_string();
        let now = chrono::Local::now().naive_local();
        let expires_at = dto
            .expires_in
            .filter(|s| *s > 0)
            .map(|s| now + chrono::Duration::seconds(s as i64));

        let mut active: sys_file::ActiveModel = file.into();
        active.public_token = Set(token.clone());
        active.public_url_expires_at = Set(expires_at);
        active.visibility = Set("PUBLIC".to_string());
        active
            .update(&self.db)
            .await
            .context("生成公开分享链接失败")?;

        Ok(FilePublicLinkVo {
            token: token.clone(),
            visibility: "PUBLIC".to_string(),
            public_url: format!("/api/public/file/{}", token),
            expires_at,
        })
    }

    /// 撤销公开分享链接
    pub async fn revoke_public_link(&self, file_id: i64) -> ApiResult<()> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let mut active: sys_file::ActiveModel = file.into();
        active.public_token = Set("".to_string());
        active.public_url_expires_at = Set(None);
        active.visibility = Set("PRIVATE".to_string());
        active
            .update(&self.db)
            .await
            .context("撤销公开分享链接失败")?;
        Ok(())
    }

    pub async fn update_visibility(
        &self,
        file_id: i64,
        dto: UpdateFileVisibilityDto,
    ) -> ApiResult<()> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let mut active: sys_file::ActiveModel = file.into();
        active.visibility = Set(dto.visibility);
        active
            .update(&self.db)
            .await
            .context("更新文件可见性失败")?;
        Ok(())
    }

    pub async fn update_status(&self, file_id: i64, dto: UpdateFileStatusDto) -> ApiResult<()> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let mut active: sys_file::ActiveModel = file.into();
        active.status = Set(dto.status);
        active.update(&self.db).await.context("更新文件状态失败")?;
        Ok(())
    }

    pub async fn update_display_name(
        &self,
        file_id: i64,
        dto: UpdateFileDisplayNameDto,
    ) -> ApiResult<()> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let mut active: sys_file::ActiveModel = file.into();
        active.display_name = Set(dto.display_name);
        active.update(&self.db).await.context("更新展示名称失败")?;
        Ok(())
    }

    pub async fn move_file(&self, file_id: i64, dto: MoveFileDto) -> ApiResult<()> {
        if let Some(folder_id) = dto.folder_id {
            let exists = sys_file_folder::Entity::find_by_id(folder_id)
                .one(&self.db)
                .await
                .context("查询文件夹失败")?
                .is_some();
            if !exists {
                return Err(ApiErrors::BadRequest("文件夹不存在".to_string()));
            }
        }

        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let mut active: sys_file::ActiveModel = file.into();
        active.folder_id = Set(dto.folder_id);
        active.update(&self.db).await.context("移动文件失败")?;
        Ok(())
    }

    /// 删除文件（软删除 + 异步清理）
    pub async fn delete_file(&self, file_id: i64, deleted_by: Option<i64>) -> ApiResult<()> {
        let file = summer_system_model::entity::sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        if file.deleted_at.is_some() {
            return Ok(());
        }

        let object_key = file.object_key.clone();
        let bucket = file.bucket.clone();

        // 先做软删除标记
        let now = chrono::Local::now().naive_local();
        let mut active: summer_system_model::entity::sys_file::ActiveModel = file.into();
        active.deleted_at = Set(Some(now));
        active.deleted_by = Set(deleted_by);
        // 先置为 NONE，后续若确认无引用再置为 PENDING 并触发异步清理
        active.purge_status = Set("NONE".to_string());
        active.purge_error = Set(None);
        active.update(&self.db).await.context("删除文件记录失败")?;

        // 检查是否还有其他记录引用同一个 S3 对象
        let ref_count = summer_system_model::entity::sys_file::Entity::find()
            .filter(summer_system_model::entity::sys_file::Column::ObjectKey.eq(&object_key))
            .filter(summer_system_model::entity::sys_file::Column::Bucket.eq(&bucket))
            .filter(summer_system_model::entity::sys_file::Column::DeletedAt.is_null())
            .count(&self.db)
            .await
            .context("查询文件引用计数失败")?;

        // 无引用时才删除 S3 对象（后台异步，失败仅记日志）
        if ref_count == 0 {
            // 标记为待清理
            summer_system_model::entity::sys_file::Entity::update_many()
                .set(summer_system_model::entity::sys_file::ActiveModel {
                    purge_status: Set("PENDING".to_string()),
                    ..Default::default()
                })
                .filter(summer_system_model::entity::sys_file::Column::Id.eq(file_id))
                .exec(&self.db)
                .await
                .context("更新文件清理状态失败")?;

            let s3 = self.s3.clone();
            let bucket = bucket.clone();
            let object_key = object_key.clone();
            let db = self.db.clone();
            self.task_queue.spawn(async move {
                let res = s3
                    .delete_object()
                    .bucket(&bucket)
                    .key(&object_key)
                    .send()
                    .await;

                let now = chrono::Local::now().naive_local();
                match res {
                    Ok(_) => {
                        let _ = summer_system_model::entity::sys_file::Entity::update_many()
                            .set(summer_system_model::entity::sys_file::ActiveModel {
                                purge_status: Set("SUCCESS".to_string()),
                                purged_at: Set(Some(now)),
                                purge_error: Set(None),
                                ..Default::default()
                            })
                            .filter(summer_system_model::entity::sys_file::Column::Id.eq(file_id))
                            .exec(&db)
                            .await;
                    }
                    Err(e) => {
                        tracing::error!(
                            bucket,
                            object_key,
                            %e,
                            "后台删除 S3 对象失败"
                        );
                        let _ = summer_system_model::entity::sys_file::Entity::update_many()
                            .set(summer_system_model::entity::sys_file::ActiveModel {
                                purge_status: Set("FAILED".to_string()),
                                purge_error: Set(Some(e.to_string())),
                                ..Default::default()
                            })
                            .filter(summer_system_model::entity::sys_file::Column::Id.eq(file_id))
                            .exec(&db)
                            .await;
                    }
                }
            });
        }

        Ok(())
    }
}
