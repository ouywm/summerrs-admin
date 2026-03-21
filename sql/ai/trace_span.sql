-- ============================================================
-- AI Span 表
-- 记录 Trace 中每个步骤，如路由、模型调用、工具调用、Guardrail、检索等
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.trace_span (
    id                  BIGSERIAL       PRIMARY KEY,
    trace_id            BIGINT          NOT NULL REFERENCES ai.trace(id) ON DELETE CASCADE,
    parent_span_id      BIGINT          NOT NULL DEFAULT 0,
    span_key            VARCHAR(64)     NOT NULL,
    span_name           VARCHAR(128)    NOT NULL DEFAULT '',
    span_type           VARCHAR(32)     NOT NULL DEFAULT 'llm',
    target_kind         VARCHAR(32)     NOT NULL DEFAULT '',
    target_ref          VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    input_payload       JSONB           NOT NULL DEFAULT '{}'::jsonb,
    output_payload      JSONB           NOT NULL DEFAULT '{}'::jsonb,
    error_message       TEXT            NOT NULL DEFAULT '',
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    started_at          TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    finished_at         TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_trace_span_trace_span_key ON ai.trace_span (trace_id, span_key);
CREATE INDEX idx_ai_trace_span_parent_span_id ON ai.trace_span (parent_span_id);
CREATE INDEX idx_ai_trace_span_status_started_at ON ai.trace_span (status, started_at);
CREATE INDEX idx_ai_trace_span_type_target ON ai.trace_span (span_type, target_kind);

COMMENT ON TABLE ai.trace_span IS 'AI Span 表';
COMMENT ON COLUMN ai.trace_span.id IS 'Span ID';
COMMENT ON COLUMN ai.trace_span.trace_id IS '追踪ID';
COMMENT ON COLUMN ai.trace_span.parent_span_id IS '父 Span ID';
COMMENT ON COLUMN ai.trace_span.span_key IS 'Span 键';
COMMENT ON COLUMN ai.trace_span.span_name IS 'Span 名称';
COMMENT ON COLUMN ai.trace_span.span_type IS 'Span 类型：llm/tool/plugin/retrieval/guardrail/router';
COMMENT ON COLUMN ai.trace_span.target_kind IS '目标类型';
COMMENT ON COLUMN ai.trace_span.target_ref IS '目标引用';
COMMENT ON COLUMN ai.trace_span.status IS '状态：1=运行中 2=成功 3=失败 4=跳过';
COMMENT ON COLUMN ai.trace_span.input_payload IS '输入载荷（JSON）';
COMMENT ON COLUMN ai.trace_span.output_payload IS '输出载荷（JSON）';
COMMENT ON COLUMN ai.trace_span.error_message IS '错误信息';
COMMENT ON COLUMN ai.trace_span.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.trace_span.started_at IS '开始时间';
COMMENT ON COLUMN ai.trace_span.finished_at IS '结束时间';
COMMENT ON COLUMN ai.trace_span.create_time IS '创建时间';
