-- ============================================================
-- AI Guardrail 命中记录表
-- 记录内容治理规则命中、拦截、脱敏等结果
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.guardrail_violation (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    token_id            BIGINT          NOT NULL DEFAULT 0,
    service_account_id  BIGINT          NOT NULL DEFAULT 0,
    rule_id             BIGINT          NOT NULL DEFAULT 0,
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    execution_id        BIGINT          NOT NULL DEFAULT 0,
    log_id              BIGINT          NOT NULL DEFAULT 0,
    task_id             BIGINT          NOT NULL DEFAULT 0,
    phase               VARCHAR(32)     NOT NULL DEFAULT '',
    category            VARCHAR(64)     NOT NULL DEFAULT '',
    action_taken        VARCHAR(32)     NOT NULL DEFAULT 'block',
    model_name          VARCHAR(128)    NOT NULL DEFAULT '',
    endpoint            VARCHAR(128)    NOT NULL DEFAULT '',
    matched_pattern     VARCHAR(512)    NOT NULL DEFAULT '',
    matched_content_hash VARCHAR(64)    NOT NULL DEFAULT '',
    sample_excerpt      TEXT            NOT NULL DEFAULT '',
    severity            SMALLINT        NOT NULL DEFAULT 2,
    latency_ms          INT             NOT NULL DEFAULT 0,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_guardrail_violation_org_time ON ai.guardrail_violation (organization_id, create_time);
CREATE INDEX idx_ai_guardrail_violation_project_time ON ai.guardrail_violation (project_id, create_time);
CREATE INDEX idx_ai_guardrail_violation_rule_id ON ai.guardrail_violation (rule_id);
CREATE INDEX idx_ai_guardrail_violation_request_id ON ai.guardrail_violation (request_id);
CREATE INDEX idx_ai_guardrail_violation_category_time ON ai.guardrail_violation (category, create_time);

COMMENT ON TABLE ai.guardrail_violation IS 'AI Guardrail 命中记录表';
COMMENT ON COLUMN ai.guardrail_violation.id IS '命中记录ID';
COMMENT ON COLUMN ai.guardrail_violation.organization_id IS '组织ID';
COMMENT ON COLUMN ai.guardrail_violation.project_id IS '项目ID';
COMMENT ON COLUMN ai.guardrail_violation.user_id IS '用户ID';
COMMENT ON COLUMN ai.guardrail_violation.token_id IS '令牌ID';
COMMENT ON COLUMN ai.guardrail_violation.service_account_id IS '服务账号ID';
COMMENT ON COLUMN ai.guardrail_violation.rule_id IS '命中的规则ID';
COMMENT ON COLUMN ai.guardrail_violation.request_id IS '关联请求ID';
COMMENT ON COLUMN ai.guardrail_violation.execution_id IS '关联执行尝试ID';
COMMENT ON COLUMN ai.guardrail_violation.log_id IS '关联消费日志ID';
COMMENT ON COLUMN ai.guardrail_violation.task_id IS '关联异步任务ID';
COMMENT ON COLUMN ai.guardrail_violation.phase IS '命中阶段';
COMMENT ON COLUMN ai.guardrail_violation.category IS '命中分类';
COMMENT ON COLUMN ai.guardrail_violation.action_taken IS '执行动作';
COMMENT ON COLUMN ai.guardrail_violation.model_name IS '关联模型名';
COMMENT ON COLUMN ai.guardrail_violation.endpoint IS '关联 endpoint';
COMMENT ON COLUMN ai.guardrail_violation.matched_pattern IS '命中的规则模式';
COMMENT ON COLUMN ai.guardrail_violation.matched_content_hash IS '命中内容哈希';
COMMENT ON COLUMN ai.guardrail_violation.sample_excerpt IS '脱敏后的命中片段';
COMMENT ON COLUMN ai.guardrail_violation.severity IS '严重级别';
COMMENT ON COLUMN ai.guardrail_violation.latency_ms IS '规则执行耗时';
COMMENT ON COLUMN ai.guardrail_violation.metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai.guardrail_violation.create_time IS '记录时间';
