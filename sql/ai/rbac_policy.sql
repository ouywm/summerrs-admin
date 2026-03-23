-- ============================================================
-- AI RBAC 策略表
-- 参考 hadrian org_rbac_policies / 组织级权限策略定义
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.rbac_policy (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL,
    scope_type          VARCHAR(32)     NOT NULL DEFAULT 'organization',
    scope_id            BIGINT          NOT NULL DEFAULT 0,
    policy_code         VARCHAR(64)     NOT NULL,
    policy_name         VARCHAR(128)    NOT NULL DEFAULT '',
    policy_type         VARCHAR(32)     NOT NULL DEFAULT 'role',
    subject_bindings    JSONB           NOT NULL DEFAULT '[]'::jsonb,
    current_version_id  BIGINT          NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    is_system           BOOLEAN         NOT NULL DEFAULT FALSE,
    description         VARCHAR(500)    NOT NULL DEFAULT '',
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_rbac_policy_scope_code ON ai.rbac_policy (organization_id, scope_type, scope_id, policy_code);
CREATE INDEX idx_ai_rbac_policy_status ON ai.rbac_policy (status);
CREATE INDEX idx_ai_rbac_policy_scope ON ai.rbac_policy (organization_id, scope_type, scope_id);

COMMENT ON TABLE ai.rbac_policy IS 'AI RBAC 策略表';
COMMENT ON COLUMN ai.rbac_policy.id IS '策略ID';
COMMENT ON COLUMN ai.rbac_policy.organization_id IS '组织ID';
COMMENT ON COLUMN ai.rbac_policy.scope_type IS '策略作用域：organization/team/project/service_account';
COMMENT ON COLUMN ai.rbac_policy.scope_id IS '策略作用域对象ID';
COMMENT ON COLUMN ai.rbac_policy.policy_code IS '策略编码';
COMMENT ON COLUMN ai.rbac_policy.policy_name IS '策略名称';
COMMENT ON COLUMN ai.rbac_policy.policy_type IS '策略类型：role/attribute/custom';
COMMENT ON COLUMN ai.rbac_policy.subject_bindings IS '策略绑定主体（JSON，如 role/team/service_account 列表）';
COMMENT ON COLUMN ai.rbac_policy.current_version_id IS '当前生效版本ID';
COMMENT ON COLUMN ai.rbac_policy.status IS '状态：1=启用 2=禁用 3=草稿';
COMMENT ON COLUMN ai.rbac_policy.is_system IS '是否系统预置策略';
COMMENT ON COLUMN ai.rbac_policy.description IS '策略说明';
COMMENT ON COLUMN ai.rbac_policy.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.rbac_policy.create_by IS '创建人';
COMMENT ON COLUMN ai.rbac_policy.create_time IS '创建时间';
COMMENT ON COLUMN ai.rbac_policy.update_by IS '更新人';
COMMENT ON COLUMN ai.rbac_policy.update_time IS '更新时间';
