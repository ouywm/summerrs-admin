//! 系统文件管理服务（列表、详情、删除）

use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_system_model::dto::sys_file::FileQueryDto;
use summer_system_model::vo::sys_file::FileVo;

use summer_plugins::background_task::BackgroundTaskQueue;
use summer_plugins::s3::S3Config;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct SysFileService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    s3: aws_sdk_s3::Client,
    #[inject(config)]
    s3_config: S3Config,
    #[inject(component)]
    task_queue: BackgroundTaskQueue,
}

impl SysFileService {
    /// 文件列表（分页）
    pub async fn list_files(
        &self,
        query: FileQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<FileVo>> {
        let page = summer_system_model::entity::sys_file::Entity::find()
            .filter(query)
            .filter(summer_system_model::entity::sys_file::Column::DeletedAt.is_null())
            .order_by_desc(summer_system_model::entity::sys_file::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询文件列表失败")?;

        let s3_config = &self.s3_config;
        Ok(page.map(|m| {
            let url = s3_config.file_url(&m.object_key);
            FileVo::from_model_with_url(m, url)
        }))
    }

    /// 文件详情
    pub async fn get_file(&self, file_id: i64) -> ApiResult<FileVo> {
        let file = summer_system_model::entity::sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        if file.deleted_at.is_some() {
            return Err(ApiErrors::NotFound("文件不存在".to_string()));
        }

        let url = self.s3_config.file_url(&file.object_key);
        Ok(FileVo::from_model_with_url(file, url))
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
        active.purge_status = Set("PENDING".to_string());
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
