-- ============================================================
-- AI 告警静默表
-- 参考 sub2api ops_alert_silences / 临时静默窗口
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.alert_silence (
    id                  BIGSERIAL       PRIMARY KEY,
    alert_rule_id       BIGINT          NOT NULL DEFAULT 0,
    scope_type          VARCHAR(32)     NOT NULL DEFAULT 'rule',
    scope_key           VARCHAR(128)    NOT NULL DEFAULT '',
    reason              VARCHAR(255)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    start_time          TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    end_time            TIMESTAMPTZ     NOT NULL,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_alert_silence_rule_id ON ai.alert_silence (alert_rule_id);
CREATE INDEX idx_ai_alert_silence_status_end_time ON ai.alert_silence (status, end_time);
CREATE INDEX idx_ai_alert_silence_scope ON ai.alert_silence (scope_type, scope_key);

COMMENT ON TABLE ai.alert_silence IS 'AI 告警静默表';
COMMENT ON COLUMN ai.alert_silence.id IS '静默ID';
COMMENT ON COLUMN ai.alert_silence.alert_rule_id IS '告警规则ID';
COMMENT ON COLUMN ai.alert_silence.scope_type IS '作用域类型';
COMMENT ON COLUMN ai.alert_silence.scope_key IS '作用域键';
COMMENT ON COLUMN ai.alert_silence.reason IS '静默原因';
COMMENT ON COLUMN ai.alert_silence.status IS '状态：1=生效中 2=已结束';
COMMENT ON COLUMN ai.alert_silence.metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai.alert_silence.create_by IS '创建人';
COMMENT ON COLUMN ai.alert_silence.start_time IS '开始时间';
COMMENT ON COLUMN ai.alert_silence.end_time IS '结束时间';
COMMENT ON COLUMN ai.alert_silence.create_time IS '创建时间';
