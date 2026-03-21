-- ============================================================
-- AI 文件表
-- 参考 hadrian files / litellm managed file 设计
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.file (
    id                  BIGSERIAL       PRIMARY KEY,
    owner_type          VARCHAR(32)     NOT NULL DEFAULT 'project',
    owner_id            BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    session_id          BIGINT          NOT NULL DEFAULT 0,
    trace_id            BIGINT          NOT NULL DEFAULT 0,
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    filename            VARCHAR(255)    NOT NULL DEFAULT '',
    purpose             VARCHAR(32)     NOT NULL DEFAULT 'assistants',
    content_type        VARCHAR(128)    NOT NULL DEFAULT '',
    size_bytes          BIGINT          NOT NULL DEFAULT 0,
    content_hash        VARCHAR(64)     NOT NULL DEFAULT '',
    storage_backend     VARCHAR(32)     NOT NULL DEFAULT 'database',
    storage_path        VARCHAR(512)    NOT NULL DEFAULT '',
    provider_file_id    VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    status_detail       TEXT            NOT NULL DEFAULT '',
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    expires_at          TIMESTAMPTZ,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_file_owner ON ai.file (owner_type, owner_id);
CREATE INDEX idx_ai_file_project_id ON ai.file (project_id);
CREATE INDEX idx_ai_file_session_id ON ai.file (session_id);
CREATE INDEX idx_ai_file_trace_id ON ai.file (trace_id);
CREATE INDEX idx_ai_file_content_hash ON ai.file (content_hash);
CREATE INDEX idx_ai_file_status ON ai.file (status);

COMMENT ON TABLE ai.file IS 'AI 文件表（上传文件、知识库文件、批处理输入文件等）';
COMMENT ON COLUMN ai.file.id IS '文件ID';
COMMENT ON COLUMN ai.file.owner_type IS '所属对象类型：platform/organization/project/user/session/trace/conversation/message';
COMMENT ON COLUMN ai.file.owner_id IS '所属对象ID';
COMMENT ON COLUMN ai.file.project_id IS '项目ID';
COMMENT ON COLUMN ai.file.session_id IS '会话ID';
COMMENT ON COLUMN ai.file.trace_id IS '追踪ID';
COMMENT ON COLUMN ai.file.request_id IS '关联请求ID';
COMMENT ON COLUMN ai.file.filename IS '文件名';
COMMENT ON COLUMN ai.file.purpose IS '文件用途：assistants/input/output/knowledge/batch/finetune';
COMMENT ON COLUMN ai.file.content_type IS '文件 MIME 类型';
COMMENT ON COLUMN ai.file.size_bytes IS '文件大小（字节）';
COMMENT ON COLUMN ai.file.content_hash IS '文件内容哈希';
COMMENT ON COLUMN ai.file.storage_backend IS '存储后端：database/s3/minio/gcs/local';
COMMENT ON COLUMN ai.file.storage_path IS '存储路径';
COMMENT ON COLUMN ai.file.provider_file_id IS '上游返回的文件ID';
COMMENT ON COLUMN ai.file.status IS '状态：1=已上传 2=处理中 3=可用 4=失败 5=删除';
COMMENT ON COLUMN ai.file.status_detail IS '状态明细';
COMMENT ON COLUMN ai.file.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.file.expires_at IS '过期时间';
COMMENT ON COLUMN ai.file.create_by IS '创建人';
COMMENT ON COLUMN ai.file.create_time IS '创建时间';
COMMENT ON COLUMN ai.file.update_by IS '更新人';
COMMENT ON COLUMN ai.file.update_time IS '更新时间';
