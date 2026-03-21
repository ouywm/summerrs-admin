-- ============================================================
-- AI 幂等记录表
-- 参考 sub2api idempotency_records
-- 用于创建任务、回调、重试写接口的去重与结果重放
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.idempotency_record (
    id                  BIGSERIAL       PRIMARY KEY,
    scope               VARCHAR(128)    NOT NULL,
    idempotency_key_hash VARCHAR(64)    NOT NULL,
    request_fingerprint VARCHAR(64)     NOT NULL,
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    status              VARCHAR(32)     NOT NULL,
    response_status     INT,
    response_body       TEXT,
    error_reason        VARCHAR(256),
    locked_until        TIMESTAMPTZ,
    expires_at          TIMESTAMPTZ     NOT NULL,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_idempotency_record_scope_key
    ON ai.idempotency_record (scope, idempotency_key_hash);
CREATE INDEX idx_ai_idempotency_record_expires_at ON ai.idempotency_record (expires_at);
CREATE INDEX idx_ai_idempotency_record_status_locked_until ON ai.idempotency_record (status, locked_until);

COMMENT ON TABLE ai.idempotency_record IS 'AI 幂等记录表（关键写接口去重与结果重放）';
COMMENT ON COLUMN ai.idempotency_record.id IS '幂等记录ID';
COMMENT ON COLUMN ai.idempotency_record.scope IS '幂等作用域（如 task.create / callback.midjourney）';
COMMENT ON COLUMN ai.idempotency_record.idempotency_key_hash IS '幂等键哈希';
COMMENT ON COLUMN ai.idempotency_record.request_fingerprint IS '请求指纹';
COMMENT ON COLUMN ai.idempotency_record.request_id IS '关联请求ID';
COMMENT ON COLUMN ai.idempotency_record.status IS '状态：processing/completed/failed 等';
COMMENT ON COLUMN ai.idempotency_record.response_status IS '已缓存的响应状态码';
COMMENT ON COLUMN ai.idempotency_record.response_body IS '已缓存的响应体';
COMMENT ON COLUMN ai.idempotency_record.error_reason IS '失败原因';
COMMENT ON COLUMN ai.idempotency_record.locked_until IS '处理锁过期时间';
COMMENT ON COLUMN ai.idempotency_record.expires_at IS '记录过期时间';
COMMENT ON COLUMN ai.idempotency_record.create_time IS '创建时间';
COMMENT ON COLUMN ai.idempotency_record.update_time IS '更新时间';
