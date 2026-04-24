-- ============================================================
-- AI 告警事件表
-- 参考 sub2api ops_alert_events / 告警触发与处理闭环
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.alert_event (
    id                  BIGSERIAL       PRIMARY KEY,
    alert_rule_id       BIGINT          NOT NULL DEFAULT 0,
    event_code          VARCHAR(64)     NOT NULL DEFAULT '',
    severity            SMALLINT        NOT NULL DEFAULT 2,
    status              SMALLINT        NOT NULL DEFAULT 1,
    source_domain       VARCHAR(32)     NOT NULL DEFAULT '',
    source_ref          VARCHAR(128)    NOT NULL DEFAULT '',
    title               VARCHAR(255)    NOT NULL DEFAULT '',
    detail              TEXT            NOT NULL DEFAULT '',
    payload             JSONB           NOT NULL DEFAULT '{}'::jsonb,
    first_triggered_at  TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    last_triggered_at   TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    ack_by              VARCHAR(64)     NOT NULL DEFAULT '',
    ack_time            TIMESTAMPTZ,
    resolved_by         VARCHAR(64)     NOT NULL DEFAULT '',
    resolved_time       TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_alert_event_rule_status ON ai.alert_event (alert_rule_id, status);
CREATE INDEX idx_ai_alert_event_severity_time ON ai.alert_event (severity, last_triggered_at);
CREATE INDEX idx_ai_alert_event_source_ref ON ai.alert_event (source_domain, source_ref);

COMMENT ON TABLE ai.alert_event IS 'AI 告警事件表';
COMMENT ON COLUMN ai.alert_event.id IS '事件ID';
COMMENT ON COLUMN ai.alert_event.alert_rule_id IS '告警规则ID';
COMMENT ON COLUMN ai.alert_event.event_code IS '事件编码';
COMMENT ON COLUMN ai.alert_event.severity IS '严重级别';
COMMENT ON COLUMN ai.alert_event.status IS '状态：1=打开 2=已确认 3=已解决 4=忽略';
COMMENT ON COLUMN ai.alert_event.source_domain IS '来源域';
COMMENT ON COLUMN ai.alert_event.source_ref IS '来源对象';
COMMENT ON COLUMN ai.alert_event.title IS '标题';
COMMENT ON COLUMN ai.alert_event.detail IS '详细说明';
COMMENT ON COLUMN ai.alert_event.payload IS '事件载荷（JSON）';
COMMENT ON COLUMN ai.alert_event.first_triggered_at IS '首次触发时间';
COMMENT ON COLUMN ai.alert_event.last_triggered_at IS '最近触发时间';
COMMENT ON COLUMN ai.alert_event.ack_by IS '确认人';
COMMENT ON COLUMN ai.alert_event.ack_time IS '确认时间';
COMMENT ON COLUMN ai.alert_event.resolved_by IS '解决人';
COMMENT ON COLUMN ai.alert_event.resolved_time IS '解决时间';
COMMENT ON COLUMN ai.alert_event.create_time IS '创建时间';
