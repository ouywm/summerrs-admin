-- ============================================================
-- AI 数据存储索引表
-- 参考 axonhub data_storage / Trace 与会话相关的对象索引
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.data_storage (
    id                  BIGSERIAL       PRIMARY KEY,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    session_id          BIGINT          NOT NULL DEFAULT 0,
    thread_id           BIGINT          NOT NULL DEFAULT 0,
    trace_id            BIGINT          NOT NULL DEFAULT 0,
    data_key            VARCHAR(128)    NOT NULL,
    data_type           VARCHAR(32)     NOT NULL DEFAULT 'json',
    storage_backend     VARCHAR(32)     NOT NULL DEFAULT 'database',
    storage_path        VARCHAR(512)    NOT NULL DEFAULT '',
    content_json        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    content_text        TEXT            NOT NULL DEFAULT '',
    content_hash        VARCHAR(64)     NOT NULL DEFAULT '',
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    status              SMALLINT        NOT NULL DEFAULT 1,
    expire_time         TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_data_storage_project_key ON ai.data_storage (project_id, data_key);
CREATE INDEX idx_ai_data_storage_session_id ON ai.data_storage (session_id);
CREATE INDEX idx_ai_data_storage_thread_id ON ai.data_storage (thread_id);
CREATE INDEX idx_ai_data_storage_trace_id ON ai.data_storage (trace_id);
CREATE INDEX idx_ai_data_storage_content_hash ON ai.data_storage (content_hash);

COMMENT ON TABLE ai.data_storage IS 'AI 数据存储索引表';
COMMENT ON COLUMN ai.data_storage.id IS '数据索引ID';
COMMENT ON COLUMN ai.data_storage.project_id IS '项目ID';
COMMENT ON COLUMN ai.data_storage.session_id IS '会话ID';
COMMENT ON COLUMN ai.data_storage.thread_id IS '线程ID';
COMMENT ON COLUMN ai.data_storage.trace_id IS '追踪ID';
COMMENT ON COLUMN ai.data_storage.data_key IS '数据键';
COMMENT ON COLUMN ai.data_storage.data_type IS '数据类型：json/text/binary/pointer';
COMMENT ON COLUMN ai.data_storage.storage_backend IS '存储后端';
COMMENT ON COLUMN ai.data_storage.storage_path IS '存储路径';
COMMENT ON COLUMN ai.data_storage.content_json IS 'JSON 内容';
COMMENT ON COLUMN ai.data_storage.content_text IS '文本内容';
COMMENT ON COLUMN ai.data_storage.content_hash IS '内容哈希';
COMMENT ON COLUMN ai.data_storage.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.data_storage.status IS '状态：1=可用 2=归档 3=删除';
COMMENT ON COLUMN ai.data_storage.expire_time IS '过期时间';
COMMENT ON COLUMN ai.data_storage.create_time IS '创建时间';
COMMENT ON COLUMN ai.data_storage.update_time IS '更新时间';
