-- ============================================================
-- AI 告警规则表
-- 参考 sub2api ops_alert_rules / 网关健康与成本告警
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.alert_rule (
    id                  BIGSERIAL       PRIMARY KEY,
    domain_code         VARCHAR(32)     NOT NULL DEFAULT 'system',
    rule_code           VARCHAR(64)     NOT NULL,
    rule_name           VARCHAR(128)    NOT NULL DEFAULT '',
    severity            SMALLINT        NOT NULL DEFAULT 2,
    metric_key          VARCHAR(128)    NOT NULL DEFAULT '',
    condition_expr      TEXT            NOT NULL DEFAULT '',
    threshold_config    JSONB           NOT NULL DEFAULT '{}'::jsonb,
    channel_config      JSONB           NOT NULL DEFAULT '{}'::jsonb,
    silence_seconds     INT             NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_alert_rule_code ON ai.alert_rule (domain_code, rule_code);
CREATE INDEX idx_ai_alert_rule_status ON ai.alert_rule (status);
CREATE INDEX idx_ai_alert_rule_metric_key ON ai.alert_rule (metric_key);

COMMENT ON TABLE ai.alert_rule IS 'AI 告警规则表';
COMMENT ON COLUMN ai.alert_rule.id IS '规则ID';
COMMENT ON COLUMN ai.alert_rule.domain_code IS '域编码';
COMMENT ON COLUMN ai.alert_rule.rule_code IS '规则编码';
COMMENT ON COLUMN ai.alert_rule.rule_name IS '规则名称';
COMMENT ON COLUMN ai.alert_rule.severity IS '严重级别';
COMMENT ON COLUMN ai.alert_rule.metric_key IS '监控指标键';
COMMENT ON COLUMN ai.alert_rule.condition_expr IS '条件表达式';
COMMENT ON COLUMN ai.alert_rule.threshold_config IS '阈值配置（JSON）';
COMMENT ON COLUMN ai.alert_rule.channel_config IS '通知渠道配置（JSON）';
COMMENT ON COLUMN ai.alert_rule.silence_seconds IS '默认静默秒数';
COMMENT ON COLUMN ai.alert_rule.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.alert_rule.create_by IS '创建人';
COMMENT ON COLUMN ai.alert_rule.create_time IS '创建时间';
COMMENT ON COLUMN ai.alert_rule.update_by IS '更新人';
COMMENT ON COLUMN ai.alert_rule.update_time IS '更新时间';
