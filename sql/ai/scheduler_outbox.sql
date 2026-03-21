-- ============================================================
-- AI 调度外发表
-- 参考 sub2api scheduler_outbox / 任务编排与可靠投递
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.scheduler_outbox (
    id                  BIGSERIAL       PRIMARY KEY,
    event_code          VARCHAR(64)     NOT NULL DEFAULT '',
    aggregate_type      VARCHAR(64)     NOT NULL DEFAULT '',
    aggregate_id        VARCHAR(128)    NOT NULL DEFAULT '',
    payload             JSONB           NOT NULL DEFAULT '{}'::jsonb,
    headers             JSONB           NOT NULL DEFAULT '{}'::jsonb,
    status              SMALLINT        NOT NULL DEFAULT 1,
    scheduled_time      TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    published_time      TIMESTAMPTZ,
    retry_count         INT             NOT NULL DEFAULT 0,
    error_message       TEXT            NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_scheduler_outbox_status_scheduled ON ai.scheduler_outbox (status, scheduled_time);
CREATE INDEX idx_ai_scheduler_outbox_aggregate ON ai.scheduler_outbox (aggregate_type, aggregate_id);
CREATE INDEX idx_ai_scheduler_outbox_event_code ON ai.scheduler_outbox (event_code);

COMMENT ON TABLE ai.scheduler_outbox IS 'AI 调度外发表';
COMMENT ON COLUMN ai.scheduler_outbox.id IS '外发任务ID';
COMMENT ON COLUMN ai.scheduler_outbox.event_code IS '事件编码';
COMMENT ON COLUMN ai.scheduler_outbox.aggregate_type IS '聚合类型';
COMMENT ON COLUMN ai.scheduler_outbox.aggregate_id IS '聚合ID';
COMMENT ON COLUMN ai.scheduler_outbox.payload IS '事件载荷（JSON）';
COMMENT ON COLUMN ai.scheduler_outbox.headers IS '附加头（JSON）';
COMMENT ON COLUMN ai.scheduler_outbox.status IS '状态：1=待发送 2=已发送 3=失败 4=取消';
COMMENT ON COLUMN ai.scheduler_outbox.scheduled_time IS '计划发送时间';
COMMENT ON COLUMN ai.scheduler_outbox.published_time IS '实际发送时间';
COMMENT ON COLUMN ai.scheduler_outbox.retry_count IS '重试次数';
COMMENT ON COLUMN ai.scheduler_outbox.error_message IS '错误信息';
COMMENT ON COLUMN ai.scheduler_outbox.create_time IS '创建时间';
COMMENT ON COLUMN ai.scheduler_outbox.update_time IS '更新时间';
