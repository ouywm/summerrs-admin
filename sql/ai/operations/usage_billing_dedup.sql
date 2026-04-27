-- ============================================================
-- AI 扣费去重表
-- 参考 sub2api usage_billing_dedup
-- 将“是否已完成扣费”从明细日志中解耦出来，避免重复结算
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.usage_billing_dedup (
    id                  BIGSERIAL       PRIMARY KEY,
    request_id          VARCHAR(64)     NOT NULL,
    token_id            BIGINT          NOT NULL DEFAULT 0,
    request_fingerprint VARCHAR(64)     NOT NULL,
    quota               BIGINT          NOT NULL DEFAULT 0,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_usage_billing_dedup_request_token
    ON ai.usage_billing_dedup (request_id, token_id);
CREATE INDEX idx_ai_usage_billing_dedup_create_time ON ai.usage_billing_dedup (create_time);

COMMENT ON TABLE ai.usage_billing_dedup IS 'AI 扣费去重表（防止同一请求重复结算）';
COMMENT ON COLUMN ai.usage_billing_dedup.id IS '去重记录ID';
COMMENT ON COLUMN ai.usage_billing_dedup.request_id IS '请求唯一标识';
COMMENT ON COLUMN ai.usage_billing_dedup.token_id IS '令牌ID';
COMMENT ON COLUMN ai.usage_billing_dedup.request_fingerprint IS '请求指纹';
COMMENT ON COLUMN ai.usage_billing_dedup.quota IS '已结算额度';
COMMENT ON COLUMN ai.usage_billing_dedup.create_time IS '创建时间';
