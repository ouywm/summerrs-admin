//! 系统文件管理服务（列表、详情、删除）

use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use model::dto::sys_file::FileQueryDto;
use model::vo::sys_file::FileVo;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;

use crate::plugin::background_task::BackgroundTaskQueue;
use crate::plugin::s3::S3Config;
use crate::plugin::sea_orm::pagination::{Page, Pagination, PaginationExt};
use crate::plugin::sea_orm::DbConn;

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
        let page = model::entity::sys_file::Entity::find()
            .filter(query)
            .order_by_desc(model::entity::sys_file::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询文件列表失败")?;

        let s3_config = &self.s3_config;
        Ok(page.map(|m| {
            let url = s3_config.file_url(&m.file_path);
            FileVo::from_model_with_url(m, url)
        }))
    }

    /// 文件详情
    pub async fn get_file(&self, file_id: i64) -> ApiResult<FileVo> {
        let file = model::entity::sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let url = self.s3_config.file_url(&file.file_path);
        Ok(FileVo::from_model_with_url(file, url))
    }

    /// 删除文件（DB 记录 + S3 对象引用计数）
    pub async fn delete_file(&self, file_id: i64) -> ApiResult<()> {
        let file = model::entity::sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let file_path = file.file_path.clone();
        let bucket = file.bucket.clone();

        // 先删 DB 记录
        model::entity::sys_file::Entity::delete_by_id(file.id)
            .exec(&self.db)
            .await
            .context("删除文件记录失败")?;

        // 检查是否还有其他记录引用同一个 S3 对象
        let ref_count = model::entity::sys_file::Entity::find()
            .filter(model::entity::sys_file::Column::FilePath.eq(&file_path))
            .filter(model::entity::sys_file::Column::Bucket.eq(&bucket))
            .count(&self.db)
            .await
            .context("查询文件引用计数失败")?;

        // 无引用时才删除 S3 对象（后台异步，失败仅记日志）
        if ref_count == 0 {
            let s3 = self.s3.clone();
            let bucket = bucket.clone();
            let file_path = file_path.clone();
            self.task_queue.spawn(async move {
                if let Err(e) = s3
                    .delete_object()
                    .bucket(&bucket)
                    .key(&file_path)
                    .send()
                    .await
                {
                    tracing::error!(
                        bucket,
                        file_path,
                        %e,
                        "后台删除 S3 对象失败"
                    );
                }
            });
        }

        Ok(())
    }
}