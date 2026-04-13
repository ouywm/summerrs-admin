//! 系统文件实体（文件中心）
//!
//! 字段设计对齐 `sql/sys/file.sql` 与参考文档 `docs/research/file-module-reference-summary.md`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "file")]
pub struct Model {
    /// 主键ID
    #[sea_orm(primary_key)]
    pub id: i64,

    // ── 业务标识 ────────────────────────────────────────────────────────────
    /// 对外业务编号
    #[sea_orm(unique)]
    pub file_no: String,

    // ── 存储定位信息 ──────────────────────────────────────────────────────
    /// 存储服务提供方（如 ALIYUN_OSS/S3/MINIO 等）
    pub provider: String,
    /// 存储桶名称
    pub bucket: String,
    /// 对象存储 key（objectKey）
    pub object_key: String,
    /// 对象存储 ETag
    pub etag: String,

    // ── 文件展示信息 ──────────────────────────────────────────────────────
    /// 上传原始文件名
    pub original_name: String,
    /// 展示文件名
    pub display_name: String,
    /// 文件扩展名（小写，不含点号）
    pub extension: String,
    /// MIME 类型
    pub mime_type: String,
    /// 文件业务分类（如 IMAGE/VIDEO/DOC 等）
    pub kind: String,
    /// 文件大小（字节）
    pub size: i64,

    // ── 媒体扩展信息 ──────────────────────────────────────────────────────
    /// 宽度（图片/视频）
    pub width: Option<i32>,
    /// 高度（图片/视频）
    pub height: Option<i32>,
    /// 时长（视频/音频）
    pub duration: Option<i32>,
    /// 页数（文档）
    pub page_count: Option<i32>,

    // ── 访问控制与分享 ────────────────────────────────────────────────────
    /// 可见性（如 PUBLIC/PRIVATE）
    pub visibility: String,
    /// 状态（如 NORMAL/DISABLED）
    pub status: String,
    /// 公开访问令牌
    pub public_token: String,
    /// 公开链接过期时间
    pub public_url_expires_at: Option<DateTime>,

    // ── 管理与扩展 ──────────────────────────────────────────────────────
    /// 标签（JSON数组）
    #[sea_orm(column_type = "JsonBinary")]
    pub tags: Json,
    /// 备注
    pub remark: String,
    /// 扩展元数据（JSON对象）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: Json,

    // ── 删除与清理 ──────────────────────────────────────────────────────
    /// 删除时间（软删除）
    pub deleted_at: Option<DateTime>,
    /// 删除人ID（对应 sys."user".id）
    pub deleted_by: Option<i64>,
    /// 清理状态（如 NONE/PENDING/RUNNING/SUCCESS/FAILED）
    pub purge_status: String,
    /// 清理完成时间
    pub purged_at: Option<DateTime>,
    /// 清理失败原因
    pub purge_error: Option<String>,

    // ── 关联信息 ────────────────────────────────────────────────────────────
    /// 文件夹ID（对应 sys.file_folder.id）
    pub folder_id: Option<i64>,
    /// 创建人ID（对应 sys."user".id）
    pub creator_id: Option<i64>,

    // ── 审计时间 ────────────────────────────────────────────────────────────
    /// 创建时间
    pub create_time: DateTime,
    /// 更新时间
    pub update_time: DateTime,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Local::now().naive_local();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
