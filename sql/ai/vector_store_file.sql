-- ============================================================
-- AI 向量库文件关联表
-- 参考 hadrian vector_store_files / 文件进入向量库的处理状态
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.vector_store_file (
    id                  BIGSERIAL       PRIMARY KEY,
    vector_store_id     BIGINT          NOT NULL REFERENCES ai.vector_store(id) ON DELETE CASCADE,
    file_id             BIGINT          NOT NULL REFERENCES ai.file(id) ON DELETE CASCADE,
    status              SMALLINT        NOT NULL DEFAULT 1,
    usage_bytes         BIGINT          NOT NULL DEFAULT 0,
    last_error          JSONB           NOT NULL DEFAULT '{}'::jsonb,
    chunking_strategy   JSONB           NOT NULL DEFAULT '{}'::jsonb,
    attributes          JSONB           NOT NULL DEFAULT '{}'::jsonb,
    deleted_at          TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_vector_store_file_unique ON ai.vector_store_file (vector_store_id, file_id);
CREATE INDEX idx_ai_vector_store_file_store_id ON ai.vector_store_file (vector_store_id);
CREATE INDEX idx_ai_vector_store_file_file_id ON ai.vector_store_file (file_id);
CREATE INDEX idx_ai_vector_store_file_status ON ai.vector_store_file (status);

COMMENT ON TABLE ai.vector_store_file IS 'AI 向量库文件关联表';
COMMENT ON COLUMN ai.vector_store_file.id IS '关联ID';
COMMENT ON COLUMN ai.vector_store_file.vector_store_id IS '向量库ID';
COMMENT ON COLUMN ai.vector_store_file.file_id IS '文件ID';
COMMENT ON COLUMN ai.vector_store_file.status IS '状态：1=处理中 2=完成 3=失败 4=取消';
COMMENT ON COLUMN ai.vector_store_file.usage_bytes IS '入库后占用字节数';
COMMENT ON COLUMN ai.vector_store_file.last_error IS '最近错误信息（JSON）';
COMMENT ON COLUMN ai.vector_store_file.chunking_strategy IS '分块策略（JSON）';
COMMENT ON COLUMN ai.vector_store_file.attributes IS '检索过滤属性（JSON）';
COMMENT ON COLUMN ai.vector_store_file.deleted_at IS '软删除时间';
COMMENT ON COLUMN ai.vector_store_file.create_time IS '创建时间';
COMMENT ON COLUMN ai.vector_store_file.update_time IS '更新时间';
