-- ============================================================
-- AI 错误透传规则表
-- 参考 sub2api error_passthrough_rules
-- 用于控制上游错误如何透传/改写给客户端
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.error_passthrough_rule (
    id                      BIGSERIAL       PRIMARY KEY,
    name                    VARCHAR(100)    NOT NULL DEFAULT '',
    enabled                 BOOLEAN         NOT NULL DEFAULT TRUE,
    priority                INT             NOT NULL DEFAULT 0,
    channel_type            SMALLINT        NOT NULL DEFAULT 0,
    vendor_code             VARCHAR(64)     NOT NULL DEFAULT '',
    error_codes             JSONB           NOT NULL DEFAULT '[]'::jsonb,
    keywords                JSONB           NOT NULL DEFAULT '[]'::jsonb,
    match_mode              VARCHAR(16)     NOT NULL DEFAULT 'any',
    passthrough_status_code BOOLEAN         NOT NULL DEFAULT TRUE,
    response_status_code    INT             NOT NULL DEFAULT 0,
    passthrough_body        BOOLEAN         NOT NULL DEFAULT TRUE,
    custom_body             TEXT            NOT NULL DEFAULT '',
    skip_monitoring         BOOLEAN         NOT NULL DEFAULT FALSE,
    description             TEXT            NOT NULL DEFAULT '',
    create_by               VARCHAR(64)     NOT NULL DEFAULT '',
    create_time             TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by               VARCHAR(64)     NOT NULL DEFAULT '',
    update_time             TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_error_passthrough_rule_enabled_priority
    ON ai.error_passthrough_rule (enabled, priority);
CREATE INDEX idx_ai_error_passthrough_rule_vendor_code ON ai.error_passthrough_rule (vendor_code);
CREATE INDEX idx_ai_error_passthrough_rule_channel_type ON ai.error_passthrough_rule (channel_type);

COMMENT ON TABLE ai.error_passthrough_rule IS 'AI 错误透传规则表（按错误码/关键词决定如何返回错误）';
COMMENT ON COLUMN ai.error_passthrough_rule.id IS '规则ID';
COMMENT ON COLUMN ai.error_passthrough_rule.name IS '规则名称';
COMMENT ON COLUMN ai.error_passthrough_rule.enabled IS '是否启用';
COMMENT ON COLUMN ai.error_passthrough_rule.priority IS '优先级（越小越先匹配）';
COMMENT ON COLUMN ai.error_passthrough_rule.channel_type IS '限定渠道类型（0=全部）';
COMMENT ON COLUMN ai.error_passthrough_rule.vendor_code IS '限定供应商编码（空=全部）';
COMMENT ON COLUMN ai.error_passthrough_rule.error_codes IS '匹配错误码列表（JSON 数组）';
COMMENT ON COLUMN ai.error_passthrough_rule.keywords IS '匹配关键词列表（JSON 数组）';
COMMENT ON COLUMN ai.error_passthrough_rule.match_mode IS '匹配模式：any/all';
COMMENT ON COLUMN ai.error_passthrough_rule.passthrough_status_code IS '是否透传上游状态码';
COMMENT ON COLUMN ai.error_passthrough_rule.response_status_code IS '自定义返回状态码';
COMMENT ON COLUMN ai.error_passthrough_rule.passthrough_body IS '是否透传上游响应体';
COMMENT ON COLUMN ai.error_passthrough_rule.custom_body IS '自定义响应体';
COMMENT ON COLUMN ai.error_passthrough_rule.skip_monitoring IS '是否跳过监控系统记录';
COMMENT ON COLUMN ai.error_passthrough_rule.description IS '规则说明';
COMMENT ON COLUMN ai.error_passthrough_rule.create_by IS '创建人';
COMMENT ON COLUMN ai.error_passthrough_rule.create_time IS '创建时间';
COMMENT ON COLUMN ai.error_passthrough_rule.update_by IS '更新人';
COMMENT ON COLUMN ai.error_passthrough_rule.update_time IS '更新时间';
