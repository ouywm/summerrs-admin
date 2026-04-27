-- ============================================================
-- AI 线程表
-- 参考 axonhub thread / 对话与工作流线程归属
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.thread (
    id                  BIGSERIAL       PRIMARY KEY,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    session_id          BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    thread_key          VARCHAR(64)     NOT NULL,
    thread_name         VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_thread_thread_key ON ai.thread (thread_key);
CREATE INDEX idx_ai_thread_project_id ON ai.thread (project_id);
CREATE INDEX idx_ai_thread_session_id ON ai.thread (session_id);
CREATE INDEX idx_ai_thread_user_id ON ai.thread (user_id);

COMMENT ON TABLE ai.thread IS 'AI 线程表';
COMMENT ON COLUMN ai.thread.id IS '线程ID';
COMMENT ON COLUMN ai.thread.project_id IS '项目ID';
COMMENT ON COLUMN ai.thread.session_id IS '会话ID';
COMMENT ON COLUMN ai.thread.user_id IS '用户ID';
COMMENT ON COLUMN ai.thread.thread_key IS '线程键';
COMMENT ON COLUMN ai.thread.thread_name IS '线程名称';
COMMENT ON COLUMN ai.thread.status IS '状态：1=活跃 2=归档 3=关闭';
COMMENT ON COLUMN ai.thread.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.thread.create_time IS '创建时间';
COMMENT ON COLUMN ai.thread.update_time IS '更新时间';
