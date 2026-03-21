-- ============================================================
-- AI Prompt 防护规则表
-- 参考 axonhub prompt_protection_rule / 提示词输入专用保护层
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.prompt_protection_rule (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    rule_code           VARCHAR(64)     NOT NULL,
    rule_name           VARCHAR(128)    NOT NULL DEFAULT '',
    pattern_type        VARCHAR(32)     NOT NULL DEFAULT 'regex',
    phase               VARCHAR(32)     NOT NULL DEFAULT 'request_input',
    action              VARCHAR(32)     NOT NULL DEFAULT 'block',
    priority            INT             NOT NULL DEFAULT 100,
    pattern_config      JSONB           NOT NULL DEFAULT '{}'::jsonb,
    rewrite_template    TEXT            NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_prompt_protection_rule_scope_code ON ai.prompt_protection_rule (organization_id, project_id, rule_code);
CREATE INDEX idx_ai_prompt_protection_rule_status_priority ON ai.prompt_protection_rule (status, priority);
CREATE INDEX idx_ai_prompt_protection_rule_pattern_type ON ai.prompt_protection_rule (pattern_type);

COMMENT ON TABLE ai.prompt_protection_rule IS 'AI Prompt 防护规则表';
COMMENT ON COLUMN ai.prompt_protection_rule.id IS '规则ID';
COMMENT ON COLUMN ai.prompt_protection_rule.organization_id IS '组织ID';
COMMENT ON COLUMN ai.prompt_protection_rule.project_id IS '项目ID';
COMMENT ON COLUMN ai.prompt_protection_rule.rule_code IS '规则编码';
COMMENT ON COLUMN ai.prompt_protection_rule.rule_name IS '规则名称';
COMMENT ON COLUMN ai.prompt_protection_rule.pattern_type IS '模式类型：regex/keyword/classifier';
COMMENT ON COLUMN ai.prompt_protection_rule.phase IS '作用阶段';
COMMENT ON COLUMN ai.prompt_protection_rule.action IS '动作：allow/block/rewrite/warn';
COMMENT ON COLUMN ai.prompt_protection_rule.priority IS '优先级';
COMMENT ON COLUMN ai.prompt_protection_rule.pattern_config IS '规则配置（JSON）';
COMMENT ON COLUMN ai.prompt_protection_rule.rewrite_template IS '改写模板';
COMMENT ON COLUMN ai.prompt_protection_rule.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.prompt_protection_rule.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.prompt_protection_rule.create_by IS '创建人';
COMMENT ON COLUMN ai.prompt_protection_rule.create_time IS '创建时间';
COMMENT ON COLUMN ai.prompt_protection_rule.update_by IS '更新人';
COMMENT ON COLUMN ai.prompt_protection_rule.update_time IS '更新时间';
