-- ============================================================
-- AI Guardrail 配置表
-- 参考 llmgateway guardrail_config / 内容治理总开关配置
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.guardrail_config (
    id                  BIGSERIAL       PRIMARY KEY,
    scope_type          VARCHAR(32)     NOT NULL DEFAULT 'organization',
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    enabled             BOOLEAN         NOT NULL DEFAULT TRUE,
    mode                VARCHAR(32)     NOT NULL DEFAULT 'enforce',
    system_rules        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    allowed_file_types  JSONB           NOT NULL DEFAULT '[]'::jsonb,
    max_file_size_mb    INT             NOT NULL DEFAULT 20,
    pii_action          VARCHAR(32)     NOT NULL DEFAULT 'redact',
    secret_action       VARCHAR(32)     NOT NULL DEFAULT 'block',
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_guardrail_config_scope ON ai.guardrail_config (scope_type, organization_id, project_id);
CREATE INDEX idx_ai_guardrail_config_org_project ON ai.guardrail_config (organization_id, project_id);
CREATE INDEX idx_ai_guardrail_config_enabled ON ai.guardrail_config (enabled);

COMMENT ON TABLE ai.guardrail_config IS 'AI Guardrail 配置表（组织/项目级内容治理开关）';
COMMENT ON COLUMN ai.guardrail_config.id IS '配置ID';
COMMENT ON COLUMN ai.guardrail_config.scope_type IS '配置作用域：platform/organization/project';
COMMENT ON COLUMN ai.guardrail_config.organization_id IS '组织ID（0=平台级）';
COMMENT ON COLUMN ai.guardrail_config.project_id IS '项目ID（0=非项目级）';
COMMENT ON COLUMN ai.guardrail_config.enabled IS '是否启用';
COMMENT ON COLUMN ai.guardrail_config.mode IS '运行模式：enforce/observe';
COMMENT ON COLUMN ai.guardrail_config.system_rules IS '系统规则配置（JSON，如 jailbreak/pii/secrets/file_types）';
COMMENT ON COLUMN ai.guardrail_config.allowed_file_types IS '允许的文件类型列表（JSON 数组）';
COMMENT ON COLUMN ai.guardrail_config.max_file_size_mb IS '文件上传大小上限（MB）';
COMMENT ON COLUMN ai.guardrail_config.pii_action IS '命中隐私信息时的动作';
COMMENT ON COLUMN ai.guardrail_config.secret_action IS '命中密钥/凭证时的动作';
COMMENT ON COLUMN ai.guardrail_config.metadata IS '扩展配置（JSON）';
COMMENT ON COLUMN ai.guardrail_config.remark IS '备注';
COMMENT ON COLUMN ai.guardrail_config.create_by IS '创建人';
COMMENT ON COLUMN ai.guardrail_config.create_time IS '创建时间';
COMMENT ON COLUMN ai.guardrail_config.update_by IS '更新人';
COMMENT ON COLUMN ai.guardrail_config.update_time IS '更新时间';
