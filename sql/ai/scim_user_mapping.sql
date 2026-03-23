-- ============================================================
-- AI SCIM 用户映射表
-- 参考 hadrian scim_user_mappings / 外部目录用户与平台用户映射
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.scim_user_mapping (
    id                  BIGSERIAL       PRIMARY KEY,
    scim_config_id      BIGINT          NOT NULL,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    external_user_id    VARCHAR(128)    NOT NULL,
    external_username   VARCHAR(255)    NOT NULL DEFAULT '',
    external_email      VARCHAR(255)    NOT NULL DEFAULT '',
    sync_direction      VARCHAR(32)     NOT NULL DEFAULT 'bidirectional',
    status              SMALLINT        NOT NULL DEFAULT 1,
    last_synced_hash    VARCHAR(64)     NOT NULL DEFAULT '',
    scim_payload        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    last_sync_at        TIMESTAMPTZ,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_scim_user_mapping_config_external_user_id ON ai.scim_user_mapping (scim_config_id, external_user_id);
CREATE INDEX idx_ai_scim_user_mapping_org_user_id ON ai.scim_user_mapping (organization_id, user_id);
CREATE INDEX idx_ai_scim_user_mapping_external_email ON ai.scim_user_mapping (external_email);

COMMENT ON TABLE ai.scim_user_mapping IS 'AI SCIM 用户映射表';
COMMENT ON COLUMN ai.scim_user_mapping.id IS '用户映射ID';
COMMENT ON COLUMN ai.scim_user_mapping.scim_config_id IS 'SCIM 配置ID';
COMMENT ON COLUMN ai.scim_user_mapping.organization_id IS '组织ID';
COMMENT ON COLUMN ai.scim_user_mapping.user_id IS '平台用户ID';
COMMENT ON COLUMN ai.scim_user_mapping.external_user_id IS '外部目录用户ID';
COMMENT ON COLUMN ai.scim_user_mapping.external_username IS '外部目录用户名';
COMMENT ON COLUMN ai.scim_user_mapping.external_email IS '外部目录邮箱';
COMMENT ON COLUMN ai.scim_user_mapping.sync_direction IS '同步方向：push/pull/bidirectional';
COMMENT ON COLUMN ai.scim_user_mapping.status IS '状态：1=正常 2=停用 3=待删除';
COMMENT ON COLUMN ai.scim_user_mapping.last_synced_hash IS '最近同步内容哈希';
COMMENT ON COLUMN ai.scim_user_mapping.scim_payload IS '最近一次 SCIM 载荷（JSON）';
COMMENT ON COLUMN ai.scim_user_mapping.last_sync_at IS '最后同步时间';
COMMENT ON COLUMN ai.scim_user_mapping.create_time IS '创建时间';
COMMENT ON COLUMN ai.scim_user_mapping.update_time IS '更新时间';
