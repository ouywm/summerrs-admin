-- ============================================================
-- AI 托管对象表
-- 参考 litellm managed object / 批处理、微调、导入导出等异步对象
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.managed_object (
    id                  BIGSERIAL       PRIMARY KEY,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    file_id             BIGINT          NOT NULL DEFAULT 0,
    vector_store_id     BIGINT          NOT NULL DEFAULT 0,
    trace_id            BIGINT          NOT NULL DEFAULT 0,
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    object_type         VARCHAR(32)     NOT NULL DEFAULT 'batch',
    provider_code       VARCHAR(64)     NOT NULL DEFAULT '',
    unified_object_key  VARCHAR(128)    NOT NULL,
    provider_object_id  VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    payload             JSONB           NOT NULL DEFAULT '{}'::jsonb,
    result              JSONB           NOT NULL DEFAULT '{}'::jsonb,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    submit_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    finish_time         TIMESTAMPTZ,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_managed_object_unified_key ON ai.managed_object (unified_object_key);
CREATE INDEX idx_ai_managed_object_project_id ON ai.managed_object (project_id);
CREATE INDEX idx_ai_managed_object_trace_id ON ai.managed_object (trace_id);
CREATE INDEX idx_ai_managed_object_type_status ON ai.managed_object (object_type, status);
CREATE INDEX idx_ai_managed_object_provider_object_id ON ai.managed_object (provider_object_id);

COMMENT ON TABLE ai.managed_object IS 'AI 托管对象表（批处理/微调/导入导出等异步对象）';
COMMENT ON COLUMN ai.managed_object.id IS '对象ID';
COMMENT ON COLUMN ai.managed_object.project_id IS '项目ID';
COMMENT ON COLUMN ai.managed_object.file_id IS '关联文件ID';
COMMENT ON COLUMN ai.managed_object.vector_store_id IS '关联向量库ID';
COMMENT ON COLUMN ai.managed_object.trace_id IS '追踪ID';
COMMENT ON COLUMN ai.managed_object.request_id IS '关联请求ID';
COMMENT ON COLUMN ai.managed_object.object_type IS '对象类型：batch/fine_tune/import/export/vector_job 等';
COMMENT ON COLUMN ai.managed_object.provider_code IS '上游提供方编码';
COMMENT ON COLUMN ai.managed_object.unified_object_key IS '统一对象键';
COMMENT ON COLUMN ai.managed_object.provider_object_id IS '上游对象ID';
COMMENT ON COLUMN ai.managed_object.status IS '状态：1=待提交 2=处理中 3=完成 4=失败 5=取消';
COMMENT ON COLUMN ai.managed_object.payload IS '提交载荷（JSON）';
COMMENT ON COLUMN ai.managed_object.result IS '处理结果（JSON）';
COMMENT ON COLUMN ai.managed_object.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.managed_object.submit_time IS '提交时间';
COMMENT ON COLUMN ai.managed_object.finish_time IS '完成时间';
COMMENT ON COLUMN ai.managed_object.create_by IS '创建人';
COMMENT ON COLUMN ai.managed_object.create_time IS '创建时间';
COMMENT ON COLUMN ai.managed_object.update_by IS '更新人';
COMMENT ON COLUMN ai.managed_object.update_time IS '更新时间';
