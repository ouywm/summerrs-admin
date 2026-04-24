-- ============================================================
-- AI SSO 组映射表
-- 参考 hadrian sso_group_mappings / 外部组到组织内角色或范围映射
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.sso_group_mapping (
    id                  BIGSERIAL       PRIMARY KEY,
    sso_config_id       BIGINT          NOT NULL,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    external_group_key  VARCHAR(255)    NOT NULL,
    external_group_name VARCHAR(255)    NOT NULL DEFAULT '',
    target_scope_type   VARCHAR(32)     NOT NULL DEFAULT 'organization',
    target_scope_id     BIGINT          NOT NULL DEFAULT 0,
    role_code           VARCHAR(64)     NOT NULL DEFAULT '',
    auto_join           BOOLEAN         NOT NULL DEFAULT TRUE,
    status              SMALLINT        NOT NULL DEFAULT 1,
    mapping_config      JSONB           NOT NULL DEFAULT '{}'::jsonb,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_sso_group_mapping_config_group_key ON ai.sso_group_mapping (sso_config_id, external_group_key);
CREATE INDEX idx_ai_sso_group_mapping_org_scope ON ai.sso_group_mapping (organization_id, target_scope_type, target_scope_id);
CREATE INDEX idx_ai_sso_group_mapping_role_code ON ai.sso_group_mapping (role_code);

COMMENT ON TABLE ai.sso_group_mapping IS 'AI SSO 组映射表';
COMMENT ON COLUMN ai.sso_group_mapping.id IS '组映射ID';
COMMENT ON COLUMN ai.sso_group_mapping.sso_config_id IS 'SSO 配置ID';
COMMENT ON COLUMN ai.sso_group_mapping.organization_id IS '组织ID';
COMMENT ON COLUMN ai.sso_group_mapping.external_group_key IS '外部组唯一键';
COMMENT ON COLUMN ai.sso_group_mapping.external_group_name IS '外部组名称';
COMMENT ON COLUMN ai.sso_group_mapping.target_scope_type IS '映射目标类型：organization/team/project';
COMMENT ON COLUMN ai.sso_group_mapping.target_scope_id IS '映射目标ID';
COMMENT ON COLUMN ai.sso_group_mapping.role_code IS '登录后赋予的角色编码';
COMMENT ON COLUMN ai.sso_group_mapping.auto_join IS '是否自动加入目标范围';
COMMENT ON COLUMN ai.sso_group_mapping.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.sso_group_mapping.mapping_config IS '映射规则配置（JSON）';
COMMENT ON COLUMN ai.sso_group_mapping.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.sso_group_mapping.create_by IS '创建人';
COMMENT ON COLUMN ai.sso_group_mapping.create_time IS '创建时间';
COMMENT ON COLUMN ai.sso_group_mapping.update_by IS '更新人';
COMMENT ON COLUMN ai.sso_group_mapping.update_time IS '更新时间';
