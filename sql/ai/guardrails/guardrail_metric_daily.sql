-- ============================================================
-- AI Guardrail 日统计表
-- 按天聚合规则执行量、拦截量、告警量等指标
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.guardrail_metric_daily (
    id                  BIGSERIAL       PRIMARY KEY,
    stats_date          DATE            NOT NULL,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    rule_id             BIGINT          NOT NULL DEFAULT 0,
    rule_code           VARCHAR(64)     NOT NULL DEFAULT '',
    requests_evaluated  BIGINT          NOT NULL DEFAULT 0,
    passed_count        BIGINT          NOT NULL DEFAULT 0,
    blocked_count       BIGINT          NOT NULL DEFAULT 0,
    redacted_count      BIGINT          NOT NULL DEFAULT 0,
    warned_count        BIGINT          NOT NULL DEFAULT 0,
    flagged_count       BIGINT          NOT NULL DEFAULT 0,
    avg_latency_ms      INT             NOT NULL DEFAULT 0,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_guardrail_metric_daily_scope ON ai.guardrail_metric_daily (stats_date, organization_id, project_id, rule_id);
CREATE INDEX idx_ai_guardrail_metric_daily_date ON ai.guardrail_metric_daily (stats_date);
CREATE INDEX idx_ai_guardrail_metric_daily_org_project ON ai.guardrail_metric_daily (organization_id, project_id);
CREATE INDEX idx_ai_guardrail_metric_daily_rule_code ON ai.guardrail_metric_daily (rule_code);

COMMENT ON TABLE ai.guardrail_metric_daily IS 'AI Guardrail 日统计表';
COMMENT ON COLUMN ai.guardrail_metric_daily.id IS '统计ID';
COMMENT ON COLUMN ai.guardrail_metric_daily.stats_date IS '统计日期';
COMMENT ON COLUMN ai.guardrail_metric_daily.organization_id IS '组织ID';
COMMENT ON COLUMN ai.guardrail_metric_daily.project_id IS '项目ID';
COMMENT ON COLUMN ai.guardrail_metric_daily.rule_id IS '规则ID';
COMMENT ON COLUMN ai.guardrail_metric_daily.rule_code IS '规则编码';
COMMENT ON COLUMN ai.guardrail_metric_daily.requests_evaluated IS '评估请求数';
COMMENT ON COLUMN ai.guardrail_metric_daily.passed_count IS '通过次数';
COMMENT ON COLUMN ai.guardrail_metric_daily.blocked_count IS '拦截次数';
COMMENT ON COLUMN ai.guardrail_metric_daily.redacted_count IS '脱敏次数';
COMMENT ON COLUMN ai.guardrail_metric_daily.warned_count IS '警告次数';
COMMENT ON COLUMN ai.guardrail_metric_daily.flagged_count IS '标记次数';
COMMENT ON COLUMN ai.guardrail_metric_daily.avg_latency_ms IS '平均执行耗时';
COMMENT ON COLUMN ai.guardrail_metric_daily.create_time IS '记录时间';
