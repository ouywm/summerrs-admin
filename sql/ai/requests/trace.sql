-- ============================================================
-- AI 追踪表
-- 参考 axonhub trace / 请求、工具、插件、检索等统一链路追踪
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.trace (
    id                  BIGSERIAL       PRIMARY KEY,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    session_id          BIGINT          NOT NULL DEFAULT 0,
    thread_id           BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    trace_key           VARCHAR(64)     NOT NULL,
    root_request_id     VARCHAR(64)     NOT NULL DEFAULT '',
    source_type         VARCHAR(32)     NOT NULL DEFAULT 'request',
    status              SMALLINT        NOT NULL DEFAULT 1,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    started_at          TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    finished_at         TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_trace_trace_key ON ai.trace (trace_key);
CREATE INDEX idx_ai_trace_project_id ON ai.trace (project_id);
CREATE INDEX idx_ai_trace_thread_id ON ai.trace (thread_id);
CREATE INDEX idx_ai_trace_session_id ON ai.trace (session_id);
CREATE INDEX idx_ai_trace_root_request_id ON ai.trace (root_request_id);
CREATE INDEX idx_ai_trace_status_started_at ON ai.trace (status, started_at);

COMMENT ON TABLE ai.trace IS 'AI 追踪表';
COMMENT ON COLUMN ai.trace.id IS '追踪ID';
COMMENT ON COLUMN ai.trace.project_id IS '项目ID';
COMMENT ON COLUMN ai.trace.session_id IS '会话ID';
COMMENT ON COLUMN ai.trace.thread_id IS '线程ID';
COMMENT ON COLUMN ai.trace.user_id IS '用户ID';
COMMENT ON COLUMN ai.trace.trace_key IS '追踪键';
COMMENT ON COLUMN ai.trace.root_request_id IS '根请求ID';
COMMENT ON COLUMN ai.trace.source_type IS '来源类型：request/task/workflow/agent';
COMMENT ON COLUMN ai.trace.status IS '状态：1=运行中 2=成功 3=失败 4=取消';
COMMENT ON COLUMN ai.trace.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.trace.started_at IS '开始时间';
COMMENT ON COLUMN ai.trace.finished_at IS '结束时间';
COMMENT ON COLUMN ai.trace.create_time IS '创建时间';
COMMENT ON COLUMN ai.trace.update_time IS '更新时间';
