CREATE SCHEMA IF NOT EXISTS sys;

-- ============================================================
-- 文件中心：文件夹表
-- 参考 docs/research/file-module-reference-summary.md
-- ============================================================

CREATE TABLE sys.file_folder (
    id          BIGSERIAL       PRIMARY KEY,
    parent_id   BIGINT          NOT NULL DEFAULT 0,
    name        VARCHAR(128)    NOT NULL,
    slug        VARCHAR(128)    NOT NULL,
    visibility  VARCHAR(32)     NOT NULL DEFAULT 'PRIVATE',
    sort        INT             NOT NULL DEFAULT 0,
    create_time TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_file_folder_parent_slug
    ON sys.file_folder (parent_id, slug);
CREATE INDEX idx_sys_file_folder_parent_sort
    ON sys.file_folder (parent_id, sort, id);
CREATE INDEX idx_sys_file_folder_visibility
    ON sys.file_folder (visibility);

COMMENT ON TABLE sys.file_folder IS '文件中心文件夹表';
COMMENT ON COLUMN sys.file_folder.id IS '主键ID';
COMMENT ON COLUMN sys.file_folder.parent_id IS '父级文件夹ID（0表示根）';
COMMENT ON COLUMN sys.file_folder.name IS '文件夹名称';
COMMENT ON COLUMN sys.file_folder.slug IS '文件夹slug（同级唯一，可用于路由/检索）';
COMMENT ON COLUMN sys.file_folder.visibility IS '可见性（如 PUBLIC/PRIVATE）';
COMMENT ON COLUMN sys.file_folder.sort IS '排序（数值越小越靠前）';
COMMENT ON COLUMN sys.file_folder.create_time IS '创建时间';
COMMENT ON COLUMN sys.file_folder.update_time IS '更新时间';

-- ============================================================
-- 文件中心：文件主表
-- 参考 docs/research/file-module-reference-summary.md
-- ============================================================

CREATE TABLE sys.file (
    id                  BIGSERIAL       PRIMARY KEY,

    -- 业务标识
    file_no             VARCHAR(64)     NOT NULL,

    -- 存储定位信息
    provider            VARCHAR(32)     NOT NULL,
    bucket              VARCHAR(128)    NOT NULL,
    object_key          VARCHAR(512)    NOT NULL,
    etag                VARCHAR(128)    NOT NULL DEFAULT '',

    -- 文件展示信息
    original_name       VARCHAR(255)    NOT NULL,
    display_name        VARCHAR(255)    NOT NULL DEFAULT '',
    extension           VARCHAR(32)     NOT NULL DEFAULT '',
    mime_type           VARCHAR(128)    NOT NULL DEFAULT '',
    kind                VARCHAR(32)     NOT NULL DEFAULT '',
    size                BIGINT          NOT NULL DEFAULT 0,
    file_md5            VARCHAR(32)     NOT NULL DEFAULT '',

    -- 媒体扩展信息
    width               INT,
    height              INT,
    duration            INT,
    page_count          INT,

    -- 访问控制信息
    visibility          VARCHAR(32)     NOT NULL DEFAULT 'PRIVATE',
    status              VARCHAR(32)     NOT NULL DEFAULT 'NORMAL',
    public_token        VARCHAR(64)     NOT NULL DEFAULT '',
    public_url_expires_at TIMESTAMP,

    -- 管理与扩展信息
    tags                JSONB           NOT NULL DEFAULT '[]'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,

    -- 删除与清理信息（软删除 + 异步物理清理）
    deleted_at          TIMESTAMP,
    deleted_by          BIGINT,
    purge_status        VARCHAR(32)     NOT NULL DEFAULT 'NONE',
    purged_at           TIMESTAMP,
    purge_error         TEXT,

    -- 关联信息
    folder_id           BIGINT,
    creator_id          BIGINT,

    -- 审计时间
    create_time         TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time         TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_file_file_no ON sys.file (file_no);
CREATE UNIQUE INDEX uk_sys_file_public_token
    ON sys.file (public_token)
    WHERE public_token <> '';

CREATE INDEX idx_sys_file_object
    ON sys.file (provider, bucket, object_key);
CREATE INDEX idx_sys_file_folder_id ON sys.file (folder_id);
CREATE INDEX idx_sys_file_creator_id ON sys.file (creator_id);
CREATE INDEX idx_sys_file_visibility_status ON sys.file (visibility, status);
CREATE INDEX idx_sys_file_md5_size
    ON sys.file (file_md5, size)
    WHERE file_md5 <> '' AND deleted_at IS NULL;
CREATE INDEX idx_sys_file_create_time ON sys.file (create_time);
CREATE INDEX idx_sys_file_deleted_at ON sys.file (deleted_at);
CREATE INDEX idx_sys_file_purge_status ON sys.file (purge_status);

COMMENT ON TABLE sys.file IS '文件中心文件表';
COMMENT ON COLUMN sys.file.id IS '主键ID';
COMMENT ON COLUMN sys.file.file_no IS '对外业务编号（稳定、可用于前端/日志/客服定位）';
COMMENT ON COLUMN sys.file.provider IS '存储服务提供方（如 ALIYUN_OSS/S3/MINIO 等）';
COMMENT ON COLUMN sys.file.bucket IS '存储桶名称';
COMMENT ON COLUMN sys.file.object_key IS '对象存储Key（objectKey）';
COMMENT ON COLUMN sys.file.etag IS '对象存储ETag';
COMMENT ON COLUMN sys.file.original_name IS '上传原始文件名';
COMMENT ON COLUMN sys.file.display_name IS '展示文件名（允许用户可见/可自定义）';
COMMENT ON COLUMN sys.file.extension IS '文件扩展名（小写，不含点号）';
COMMENT ON COLUMN sys.file.mime_type IS 'MIME类型';
COMMENT ON COLUMN sys.file.kind IS '文件业务分类（如 IMAGE/VIDEO/DOC 等）';
COMMENT ON COLUMN sys.file.size IS '文件大小（字节）';
COMMENT ON COLUMN sys.file.file_md5 IS '文件内容MD5（32位小写hex，用于内容去重/秒传）';
COMMENT ON COLUMN sys.file.width IS '宽度（图片/视频）';
COMMENT ON COLUMN sys.file.height IS '高度（图片/视频）';
COMMENT ON COLUMN sys.file.duration IS '时长（视频/音频/媒体类）';
COMMENT ON COLUMN sys.file.page_count IS '页数（文档类）';
COMMENT ON COLUMN sys.file.visibility IS '可见性（如 PUBLIC/PRIVATE）';
COMMENT ON COLUMN sys.file.status IS '状态（如 NORMAL/DISABLED 等）';
COMMENT ON COLUMN sys.file.public_token IS '公开访问令牌（用于生成分享链接）';
COMMENT ON COLUMN sys.file.public_url_expires_at IS '公开链接过期时间';
COMMENT ON COLUMN sys.file.tags IS '标签（JSON数组）';
COMMENT ON COLUMN sys.file.remark IS '备注';
COMMENT ON COLUMN sys.file.metadata IS '扩展元数据（JSON对象）';
COMMENT ON COLUMN sys.file.deleted_at IS '删除时间（软删除标记）';
COMMENT ON COLUMN sys.file.deleted_by IS '删除人ID（对应 sys.\"user\".id）';
COMMENT ON COLUMN sys.file.purge_status IS '清理状态（NONE/PENDING/RUNNING/SUCCESS/FAILED 等）';
COMMENT ON COLUMN sys.file.purged_at IS '物理清理完成时间';
COMMENT ON COLUMN sys.file.purge_error IS '物理清理失败原因';
COMMENT ON COLUMN sys.file.folder_id IS '文件夹ID（对应 sys.file_folder.id）';
COMMENT ON COLUMN sys.file.creator_id IS '创建人ID（对应 sys.\"user\".id）';
COMMENT ON COLUMN sys.file.create_time IS '创建时间';
COMMENT ON COLUMN sys.file.update_time IS '更新时间';
