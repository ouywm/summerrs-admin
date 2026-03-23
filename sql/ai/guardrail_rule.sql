-- ============================================================
-- AI Guardrail 规则表
-- 参考 llmgateway guardrail_rule / 自定义内容治理规则
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.guardrail_rule (
    id                  BIGSERIAL       PRIMARY KEY,
    guardrail_config_id BIGINT          NOT NULL,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    team_id             BIGINT          NOT NULL DEFAULT 0,
    token_id            BIGINT          NOT NULL DEFAULT 0,
    service_account_id  BIGINT          NOT NULL DEFAULT 0,
    rule_code           VARCHAR(64)     NOT NULL,
    rule_name           VARCHAR(128)    NOT NULL DEFAULT '',
    rule_type           VARCHAR(32)     NOT NULL DEFAULT 'custom_regex',
    phase               VARCHAR(32)     NOT NULL DEFAULT 'request_input',
    action              VARCHAR(32)     NOT NULL DEFAULT 'block',
    priority            INT             NOT NULL DEFAULT 100,
    enabled             BOOLEAN         NOT NULL DEFAULT TRUE,
    severity            SMALLINT        NOT NULL DEFAULT 2,
    model_pattern       VARCHAR(128)    NOT NULL DEFAULT '*',
    endpoint_pattern    VARCHAR(128)    NOT NULL DEFAULT '*',
    condition_json      JSONB           NOT NULL DEFAULT '{}'::jsonb,
    rule_config         JSONB           NOT NULL DEFAULT '{}'::jsonb,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_guardrail_rule_config_code ON ai.guardrail_rule (guardrail_config_id, rule_code);
CREATE INDEX idx_ai_guardrail_rule_org_project ON ai.guardrail_rule (organization_id, project_id);
CREATE INDEX idx_ai_guardrail_rule_priority_enabled ON ai.guardrail_rule (priority, enabled);
CREATE INDEX idx_ai_guardrail_rule_phase ON ai.guardrail_rule (phase);

COMMENT ON TABLE ai.guardrail_rule IS 'AI Guardrail 规则表（自定义/系统内容治理规则）';
COMMENT ON COLUMN ai.guardrail_rule.id IS '规则ID';
COMMENT ON COLUMN ai.guardrail_rule.guardrail_config_id IS '所属 Guardrail 配置ID';
COMMENT ON COLUMN ai.guardrail_rule.organization_id IS '组织ID';
COMMENT ON COLUMN ai.guardrail_rule.project_id IS '项目ID';
COMMENT ON COLUMN ai.guardrail_rule.team_id IS '团队ID';
COMMENT ON COLUMN ai.guardrail_rule.token_id IS '令牌ID（0=不绑定）';
COMMENT ON COLUMN ai.guardrail_rule.service_account_id IS '服务账号ID（0=不绑定）';
COMMENT ON COLUMN ai.guardrail_rule.rule_code IS '规则编码';
COMMENT ON COLUMN ai.guardrail_rule.rule_name IS '规则名称';
COMMENT ON COLUMN ai.guardrail_rule.rule_type IS '规则类型：blocked_terms/custom_regex/topic_restriction/pii/prompt_injection/file_types 等';
COMMENT ON COLUMN ai.guardrail_rule.phase IS '执行阶段：request_input/response_output/file_upload/tool_result/system_prompt';
COMMENT ON COLUMN ai.guardrail_rule.action IS '命中后的动作：allow/block/redact/warn/quarantine';
COMMENT ON COLUMN ai.guardrail_rule.priority IS '优先级（越大越先执行）';
COMMENT ON COLUMN ai.guardrail_rule.enabled IS '是否启用';
COMMENT ON COLUMN ai.guardrail_rule.severity IS '严重级别：1=低 2=中 3=高';
COMMENT ON COLUMN ai.guardrail_rule.model_pattern IS '模型匹配模式';
COMMENT ON COLUMN ai.guardrail_rule.endpoint_pattern IS 'Endpoint 匹配模式';
COMMENT ON COLUMN ai.guardrail_rule.condition_json IS '附加条件（JSON）';
COMMENT ON COLUMN ai.guardrail_rule.rule_config IS '规则配置（JSON）';
COMMENT ON COLUMN ai.guardrail_rule.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.guardrail_rule.remark IS '备注';
COMMENT ON COLUMN ai.guardrail_rule.create_by IS '创建人';
COMMENT ON COLUMN ai.guardrail_rule.create_time IS '创建时间';
COMMENT ON COLUMN ai.guardrail_rule.update_by IS '更新人';
COMMENT ON COLUMN ai.guardrail_rule.update_time IS '更新时间';
