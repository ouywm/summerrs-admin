-- ============================================================
-- AI SCIM 组映射表
-- 参考 hadrian scim_group_mappings / 外部目录组与平台范围映射
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.scim_group_mapping (
    id                  BIGSERIAL       PRIMARY KEY,
    scim_config_id      BIGINT          NOT NULL,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    external_group_id   VARCHAR(128)    NOT NULL,
    external_group_name VARCHAR(255)    NOT NULL DEFAULT '',
    target_scope_type   VARCHAR(32)     NOT NULL DEFAULT 'team',
    target_scope_id     BIGINT          NOT NULL DEFAULT 0,
    role_code           VARCHAR(64)     NOT NULL DEFAULT '',
    sync_direction      VARCHAR(32)     NOT NULL DEFAULT 'bidirectional',
    status              SMALLINT        NOT NULL DEFAULT 1,
    scim_payload        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    last_sync_at        TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_scim_group_mapping_config_external_group_id ON ai.scim_group_mapping (scim_config_id, external_group_id);
CREATE INDEX idx_ai_scim_group_mapping_org_scope ON ai.scim_group_mapping (organization_id, target_scope_type, target_scope_id);
CREATE INDEX idx_ai_scim_group_mapping_role_code ON ai.scim_group_mapping (role_code);

COMMENT ON TABLE ai.scim_group_mapping IS 'AI SCIM 组映射表';
COMMENT ON COLUMN ai.scim_group_mapping.id IS '组映射ID';
COMMENT ON COLUMN ai.scim_group_mapping.scim_config_id IS 'SCIM 配置ID';
COMMENT ON COLUMN ai.scim_group_mapping.organization_id IS '组织ID';
COMMENT ON COLUMN ai.scim_group_mapping.external_group_id IS '外部目录组ID';
COMMENT ON COLUMN ai.scim_group_mapping.external_group_name IS '外部目录组名称';
COMMENT ON COLUMN ai.scim_group_mapping.target_scope_type IS '目标范围类型：organization/team/project';
COMMENT ON COLUMN ai.scim_group_mapping.target_scope_id IS '目标范围ID';
COMMENT ON COLUMN ai.scim_group_mapping.role_code IS '映射后的角色编码';
COMMENT ON COLUMN ai.scim_group_mapping.sync_direction IS '同步方向：push/pull/bidirectional';
COMMENT ON COLUMN ai.scim_group_mapping.status IS '状态：1=正常 2=停用 3=待删除';
COMMENT ON COLUMN ai.scim_group_mapping.scim_payload IS '最近一次 SCIM 载荷（JSON）';
COMMENT ON COLUMN ai.scim_group_mapping.last_sync_at IS '最后同步时间';
COMMENT ON COLUMN ai.scim_group_mapping.create_time IS '创建时间';
COMMENT ON COLUMN ai.scim_group_mapping.update_time IS '更新时间';
