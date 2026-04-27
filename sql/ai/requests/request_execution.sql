-- ============================================================
-- AI 请求执行尝试表
-- 一次 ai.request 可能会重试多个渠道/多个账号
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.request_execution (
    id                  BIGSERIAL       PRIMARY KEY,
    ai_request_id       BIGINT          NOT NULL,
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    attempt_no          INT             NOT NULL DEFAULT 1,
    channel_id          BIGINT          NOT NULL DEFAULT 0,
    account_id          BIGINT          NOT NULL DEFAULT 0,
    endpoint            VARCHAR(64)     NOT NULL DEFAULT '/v1/chat/completions',
    request_format      VARCHAR(64)     NOT NULL DEFAULT 'openai/chat_completions',
    requested_model     VARCHAR(128)    NOT NULL DEFAULT '',
    upstream_model      VARCHAR(128)    NOT NULL DEFAULT '',
    upstream_request_id VARCHAR(128)    NOT NULL DEFAULT '',
    request_headers     JSONB           NOT NULL DEFAULT '{}'::jsonb,
    request_body        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    response_body       JSONB,
    response_status_code INT            NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    error_message       TEXT            NOT NULL DEFAULT '',
    duration_ms         INT             NOT NULL DEFAULT 0,
    first_token_ms      INT             NOT NULL DEFAULT 0,
    started_at          TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    finished_at         TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_request_execution_request_attempt
    ON ai.request_execution (ai_request_id, attempt_no);
CREATE INDEX idx_ai_request_execution_request_id ON ai.request_execution (request_id);
CREATE INDEX idx_ai_request_execution_channel_id ON ai.request_execution (channel_id);
CREATE INDEX idx_ai_request_execution_account_id ON ai.request_execution (account_id);
CREATE INDEX idx_ai_request_execution_status_started_at ON ai.request_execution (status, started_at);

COMMENT ON TABLE ai.request_execution IS 'AI 请求执行尝试表（一次请求的每次上游尝试）';
COMMENT ON COLUMN ai.request_execution.id IS '执行尝试ID';
COMMENT ON COLUMN ai.request_execution.ai_request_id IS '所属请求主键（ai.request.id）';
COMMENT ON COLUMN ai.request_execution.request_id IS '请求唯一标识冗余';
COMMENT ON COLUMN ai.request_execution.attempt_no IS '第几次尝试（从1开始）';
COMMENT ON COLUMN ai.request_execution.channel_id IS '命中的渠道ID';
COMMENT ON COLUMN ai.request_execution.account_id IS '命中的账号ID';
COMMENT ON COLUMN ai.request_execution.endpoint IS '此次尝试的 endpoint';
COMMENT ON COLUMN ai.request_execution.request_format IS '此次尝试的上游协议格式';
COMMENT ON COLUMN ai.request_execution.requested_model IS '客户端请求模型';
COMMENT ON COLUMN ai.request_execution.upstream_model IS '转发给上游的模型';
COMMENT ON COLUMN ai.request_execution.upstream_request_id IS '上游请求ID';
COMMENT ON COLUMN ai.request_execution.request_headers IS '上游请求头快照（脱敏后）';
COMMENT ON COLUMN ai.request_execution.request_body IS '发给上游的真实请求体';
COMMENT ON COLUMN ai.request_execution.response_body IS '上游返回的响应体（非流式或摘要）';
COMMENT ON COLUMN ai.request_execution.response_status_code IS '上游状态码';
COMMENT ON COLUMN ai.request_execution.status IS '状态：1=待执行 2=执行中 3=成功 4=失败 5=取消';
COMMENT ON COLUMN ai.request_execution.error_message IS '失败摘要';
COMMENT ON COLUMN ai.request_execution.duration_ms IS '此次尝试耗时（毫秒）';
COMMENT ON COLUMN ai.request_execution.first_token_ms IS '此次尝试首 token 延迟（毫秒）';
COMMENT ON COLUMN ai.request_execution.started_at IS '开始时间';
COMMENT ON COLUMN ai.request_execution.finished_at IS '结束时间';
COMMENT ON COLUMN ai.request_execution.create_time IS '记录创建时间';
