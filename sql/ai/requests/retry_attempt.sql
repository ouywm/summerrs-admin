-- ============================================================
-- AI 重试记录表
-- 记录失败任务的每次重试行为
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.retry_attempt (
    id                  BIGSERIAL       PRIMARY KEY,
    domain_code         VARCHAR(32)     NOT NULL DEFAULT '',
    task_type           VARCHAR(64)     NOT NULL DEFAULT '',
    reference_id        VARCHAR(128)    NOT NULL DEFAULT '',
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    attempt_no          INT             NOT NULL DEFAULT 1,
    status              SMALLINT        NOT NULL DEFAULT 1,
    backoff_seconds     INT             NOT NULL DEFAULT 0,
    error_message       TEXT            NOT NULL DEFAULT '',
    payload             JSONB           NOT NULL DEFAULT '{}'::jsonb,
    next_retry_at       TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_retry_attempt_unique ON ai.retry_attempt (domain_code, task_type, reference_id, attempt_no);
CREATE INDEX idx_ai_retry_attempt_next_retry ON ai.retry_attempt (status, next_retry_at);
CREATE INDEX idx_ai_retry_attempt_request_id ON ai.retry_attempt (request_id);

COMMENT ON TABLE ai.retry_attempt IS 'AI 重试记录表';
COMMENT ON COLUMN ai.retry_attempt.id IS '重试记录ID';
COMMENT ON COLUMN ai.retry_attempt.domain_code IS '域编码';
COMMENT ON COLUMN ai.retry_attempt.task_type IS '任务类型';
COMMENT ON COLUMN ai.retry_attempt.reference_id IS '关联对象标识';
COMMENT ON COLUMN ai.retry_attempt.request_id IS '关联请求ID';
COMMENT ON COLUMN ai.retry_attempt.attempt_no IS '第几次重试';
COMMENT ON COLUMN ai.retry_attempt.status IS '状态：1=待重试 2=成功 3=失败 4=放弃';
COMMENT ON COLUMN ai.retry_attempt.backoff_seconds IS '退避秒数';
COMMENT ON COLUMN ai.retry_attempt.error_message IS '错误信息';
COMMENT ON COLUMN ai.retry_attempt.payload IS '重试载荷（JSON）';
COMMENT ON COLUMN ai.retry_attempt.next_retry_at IS '下次重试时间';
COMMENT ON COLUMN ai.retry_attempt.create_time IS '创建时间';
COMMENT ON COLUMN ai.retry_attempt.update_time IS '更新时间';
