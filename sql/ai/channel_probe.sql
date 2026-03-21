-- ============================================================
-- AI 渠道健康检查日志表
-- 记录渠道或账号的手动测速 / 自动探测 / 自动恢复结果
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.channel_probe (
    id                BIGSERIAL       PRIMARY KEY,
    channel_id        BIGINT          NOT NULL,
    account_id        BIGINT          NOT NULL DEFAULT 0,
    request_id        VARCHAR(64)     NOT NULL DEFAULT '',
    probe_type        SMALLINT        NOT NULL DEFAULT 1,
    test_model        VARCHAR(128)    NOT NULL DEFAULT '',
    status            SMALLINT        NOT NULL DEFAULT 1,
    response_time     INT             NOT NULL DEFAULT 0,
    first_token_time  INT             NOT NULL DEFAULT 0,
    status_code       INT             NOT NULL DEFAULT 0,
    error_message     TEXT            NOT NULL DEFAULT '',
    request_body      JSONB,
    response_body     JSONB,
    create_time       TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_channel_probe_channel_id ON ai.channel_probe (channel_id);
CREATE INDEX idx_ai_channel_probe_account_id ON ai.channel_probe (account_id);
CREATE INDEX idx_ai_channel_probe_status ON ai.channel_probe (status);
CREATE INDEX idx_ai_channel_probe_create_time ON ai.channel_probe (create_time);

COMMENT ON TABLE ai.channel_probe IS 'AI 渠道健康检查日志表（测速与可用性检测结果）';
COMMENT ON COLUMN ai.channel_probe.id IS '检查ID';
COMMENT ON COLUMN ai.channel_probe.channel_id IS '渠道ID';
COMMENT ON COLUMN ai.channel_probe.account_id IS '账号ID（0 表示只测渠道级）';
COMMENT ON COLUMN ai.channel_probe.request_id IS '检查请求ID';
COMMENT ON COLUMN ai.channel_probe.probe_type IS '检查类型：1=手动测速 2=定时检查 3=故障后自动恢复';
COMMENT ON COLUMN ai.channel_probe.test_model IS '测试模型';
COMMENT ON COLUMN ai.channel_probe.status IS '检查结果：1=成功 2=失败 3=超时';
COMMENT ON COLUMN ai.channel_probe.response_time IS '总响应时间（毫秒）';
COMMENT ON COLUMN ai.channel_probe.first_token_time IS '首 token 时间（毫秒）';
COMMENT ON COLUMN ai.channel_probe.status_code IS 'HTTP 状态码';
COMMENT ON COLUMN ai.channel_probe.error_message IS '错误摘要';
COMMENT ON COLUMN ai.channel_probe.request_body IS '测试请求体';
COMMENT ON COLUMN ai.channel_probe.response_body IS '测试响应体摘要';
COMMENT ON COLUMN ai.channel_probe.create_time IS '检查时间';
