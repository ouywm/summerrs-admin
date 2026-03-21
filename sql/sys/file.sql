CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.file (
    id              BIGSERIAL       PRIMARY KEY,
    file_name       VARCHAR(255)    NOT NULL,
    original_name   VARCHAR(255)    NOT NULL,
    file_path       VARCHAR(512)    NOT NULL,
    file_size       BIGINT          NOT NULL DEFAULT 0,
    file_suffix     VARCHAR(32)     NOT NULL DEFAULT '',
    mime_type       VARCHAR(128)    NOT NULL DEFAULT '',
    bucket          VARCHAR(128)    NOT NULL,
    file_md5        VARCHAR(64)     NOT NULL DEFAULT '',
    upload_by       VARCHAR(64)     NOT NULL DEFAULT '',
    upload_by_id    BIGINT,
    create_time     TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sys_file_bucket ON sys.file (bucket);
CREATE INDEX idx_sys_file_upload_by_id ON sys.file (upload_by_id);
CREATE INDEX idx_sys_file_create_time ON sys.file (create_time);
CREATE INDEX idx_sys_file_file_md5 ON sys.file (file_md5);

COMMENT ON TABLE sys.file IS '系统文件表';
COMMENT ON COLUMN sys.file.id IS '主键 ID';
COMMENT ON COLUMN sys.file.file_name IS '存储文件名';
COMMENT ON COLUMN sys.file.original_name IS '用户上传的原始文件名';
COMMENT ON COLUMN sys.file.file_path IS 'S3 对象 key';
COMMENT ON COLUMN sys.file.file_size IS '文件大小（字节）';
COMMENT ON COLUMN sys.file.file_suffix IS '文件后缀（小写，不含点号）';
COMMENT ON COLUMN sys.file.mime_type IS 'MIME 类型';
COMMENT ON COLUMN sys.file.bucket IS '存储桶名称';
COMMENT ON COLUMN sys.file.file_md5 IS '文件 MD5 摘要，用于秒传判重';
COMMENT ON COLUMN sys.file.upload_by IS '上传人昵称';
COMMENT ON COLUMN sys.file.upload_by_id IS '上传人 ID';
COMMENT ON COLUMN sys.file.create_time IS '创建时间';
