-- ============================================================
-- AI 使用记录清理任务表
-- 参考 sub2api usage_cleanup_tasks / 历史账单和日志清理任务
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.usage_cleanup_task (
    id                  BIGSERIAL       PRIMARY KEY,
    task_no             VARCHAR(64)     NOT NULL,
    status              SMALLINT        NOT NULL DEFAULT 1,
    target_table        VARCHAR(64)     NOT NULL DEFAULT 'ai.log',
    filters             JSONB           NOT NULL DEFAULT '{}'::jsonb,
    deleted_rows        BIGINT          NOT NULL DEFAULT 0,
    error_message       TEXT            NOT NULL DEFAULT '',
    started_by          BIGINT          NOT NULL DEFAULT 0,
    canceled_by         BIGINT          NOT NULL DEFAULT 0,
    canceled_at         TIMESTAMPTZ,
    started_at          TIMESTAMPTZ,
    finished_at         TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_usage_cleanup_task_task_no ON ai.usage_cleanup_task (task_no);
CREATE INDEX idx_ai_usage_cleanup_task_status_time ON ai.usage_cleanup_task (status, create_time);
CREATE INDEX idx_ai_usage_cleanup_task_finished_at ON ai.usage_cleanup_task (finished_at);

COMMENT ON TABLE ai.usage_cleanup_task IS 'AI 使用记录清理任务表';
COMMENT ON COLUMN ai.usage_cleanup_task.id IS '任务ID';
COMMENT ON COLUMN ai.usage_cleanup_task.task_no IS '任务编号';
COMMENT ON COLUMN ai.usage_cleanup_task.status IS '状态：1=待执行 2=执行中 3=成功 4=失败 5=取消';
COMMENT ON COLUMN ai.usage_cleanup_task.target_table IS '目标清理表';
COMMENT ON COLUMN ai.usage_cleanup_task.filters IS '清理过滤条件（JSON）';
COMMENT ON COLUMN ai.usage_cleanup_task.deleted_rows IS '已删除行数';
COMMENT ON COLUMN ai.usage_cleanup_task.error_message IS '错误信息';
COMMENT ON COLUMN ai.usage_cleanup_task.started_by IS '执行人';
COMMENT ON COLUMN ai.usage_cleanup_task.canceled_by IS '取消人';
COMMENT ON COLUMN ai.usage_cleanup_task.canceled_at IS '取消时间';
COMMENT ON COLUMN ai.usage_cleanup_task.started_at IS '开始时间';
COMMENT ON COLUMN ai.usage_cleanup_task.finished_at IS '结束时间';
COMMENT ON COLUMN ai.usage_cleanup_task.create_time IS '创建时间';
COMMENT ON COLUMN ai.usage_cleanup_task.update_time IS '更新时间';
