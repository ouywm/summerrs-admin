-- ============================================================
-- AI 向量库表
-- 参考 hadrian vector_stores / RAG 知识库容器
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.vector_store (
    id                  BIGSERIAL       PRIMARY KEY,
    owner_type          VARCHAR(32)     NOT NULL DEFAULT 'project',
    owner_id            BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    name                VARCHAR(255)    NOT NULL DEFAULT '',
    description         TEXT            NOT NULL DEFAULT '',
    embedding_model     VARCHAR(128)    NOT NULL DEFAULT 'text-embedding-3-small',
    embedding_dimensions INT            NOT NULL DEFAULT 1536,
    storage_backend     VARCHAR(32)     NOT NULL DEFAULT 'pgvector',
    provider_vector_store_id VARCHAR(128) NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    usage_bytes         BIGINT          NOT NULL DEFAULT 0,
    file_counts         JSONB           NOT NULL DEFAULT '{"cancelled":0,"completed":0,"failed":0,"in_progress":0,"total":0}'::jsonb,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    expires_after       JSONB           NOT NULL DEFAULT '{}'::jsonb,
    expires_at          TIMESTAMPTZ,
    last_active_at      TIMESTAMPTZ,
    deleted_at          TIMESTAMPTZ,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_vector_store_owner_name ON ai.vector_store (owner_type, owner_id, name);
CREATE INDEX idx_ai_vector_store_project_id ON ai.vector_store (project_id);
CREATE INDEX idx_ai_vector_store_status ON ai.vector_store (status);
CREATE INDEX idx_ai_vector_store_embedding_model ON ai.vector_store (embedding_model);
CREATE INDEX idx_ai_vector_store_expires_at ON ai.vector_store (expires_at);

COMMENT ON TABLE ai.vector_store IS 'AI 向量库表（RAG 知识库容器）';
COMMENT ON COLUMN ai.vector_store.id IS '向量库ID';
COMMENT ON COLUMN ai.vector_store.owner_type IS '所属对象类型';
COMMENT ON COLUMN ai.vector_store.owner_id IS '所属对象ID';
COMMENT ON COLUMN ai.vector_store.project_id IS '项目ID';
COMMENT ON COLUMN ai.vector_store.name IS '向量库名称';
COMMENT ON COLUMN ai.vector_store.description IS '描述';
COMMENT ON COLUMN ai.vector_store.embedding_model IS 'Embedding 模型';
COMMENT ON COLUMN ai.vector_store.embedding_dimensions IS 'Embedding 维度';
COMMENT ON COLUMN ai.vector_store.storage_backend IS '向量存储后端：pgvector/qdrant/weaviate/milvus';
COMMENT ON COLUMN ai.vector_store.provider_vector_store_id IS '上游向量库ID';
COMMENT ON COLUMN ai.vector_store.status IS '状态：1=可用 2=处理中 3=失败 4=归档';
COMMENT ON COLUMN ai.vector_store.usage_bytes IS '占用字节数';
COMMENT ON COLUMN ai.vector_store.file_counts IS '文件统计（JSON）';
COMMENT ON COLUMN ai.vector_store.metadata IS '元数据（JSON）';
COMMENT ON COLUMN ai.vector_store.expires_after IS '过期策略（JSON）';
COMMENT ON COLUMN ai.vector_store.expires_at IS '过期时间';
COMMENT ON COLUMN ai.vector_store.last_active_at IS '最后活跃时间';
COMMENT ON COLUMN ai.vector_store.deleted_at IS '软删除时间';
COMMENT ON COLUMN ai.vector_store.create_by IS '创建人';
COMMENT ON COLUMN ai.vector_store.create_time IS '创建时间';
COMMENT ON COLUMN ai.vector_store.update_by IS '更新人';
COMMENT ON COLUMN ai.vector_store.update_time IS '更新时间';
