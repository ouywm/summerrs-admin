-- ============================================================
-- AI 死信队列表
-- 参考 hadrian dead_letter_queue / 失败任务兜底存储
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.dead_letter_queue (
    id                  BIGSERIAL       PRIMARY KEY,
    entry_type          VARCHAR(64)     NOT NULL DEFAULT '',
    source_domain       VARCHAR(32)     NOT NULL DEFAULT '',
    reference_id        VARCHAR(128)    NOT NULL DEFAULT '',
    payload             JSONB           NOT NULL DEFAULT '{}'::jsonb,
    error_message       TEXT            NOT NULL DEFAULT '',
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    retry_count         INT             NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    available_at        TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    last_retry_at       TIMESTAMPTZ,
    resolved_at         TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_dead_letter_queue_status_available ON ai.dead_letter_queue (status, available_at);
CREATE INDEX idx_ai_dead_letter_queue_entry_type ON ai.dead_letter_queue (entry_type);
CREATE INDEX idx_ai_dead_letter_queue_reference_id ON ai.dead_letter_queue (reference_id);

COMMENT ON TABLE ai.dead_letter_queue IS 'AI 死信队列表';
COMMENT ON COLUMN ai.dead_letter_queue.id IS '死信ID';
COMMENT ON COLUMN ai.dead_letter_queue.entry_type IS '死信类型';
COMMENT ON COLUMN ai.dead_letter_queue.source_domain IS '来源域：relay/guardrail/file/payment/webhook/scheduler';
COMMENT ON COLUMN ai.dead_letter_queue.reference_id IS '来源对象标识';
COMMENT ON COLUMN ai.dead_letter_queue.payload IS '原始载荷（JSON）';
COMMENT ON COLUMN ai.dead_letter_queue.error_message IS '失败原因';
COMMENT ON COLUMN ai.dead_letter_queue.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.dead_letter_queue.retry_count IS '重试次数';
COMMENT ON COLUMN ai.dead_letter_queue.status IS '状态：1=待处理 2=重试中 3=已解决 4=放弃';
COMMENT ON COLUMN ai.dead_letter_queue.available_at IS '下次可处理时间';
COMMENT ON COLUMN ai.dead_letter_queue.last_retry_at IS '最近重试时间';
COMMENT ON COLUMN ai.dead_letter_queue.resolved_at IS '解决时间';
COMMENT ON COLUMN ai.dead_letter_queue.create_time IS '创建时间';
