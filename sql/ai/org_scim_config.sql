-- ============================================================
-- AI 组织 SCIM 配置表
-- 参考 hadrian org_scim_configs / 企业目录同步配置
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.org_scim_config (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL REFERENCES ai.organization(id) ON DELETE CASCADE,
    provider_code       VARCHAR(64)     NOT NULL DEFAULT 'default',
    base_url            VARCHAR(512)    NOT NULL DEFAULT '',
    auth_type           VARCHAR(32)     NOT NULL DEFAULT 'bearer',
    bearer_token_ref    VARCHAR(255)    NOT NULL DEFAULT '',
    provisioning_mode   VARCHAR(32)     NOT NULL DEFAULT 'push',
    sync_interval_minutes INT           NOT NULL DEFAULT 60,
    user_sync_enabled   BOOLEAN         NOT NULL DEFAULT TRUE,
    group_sync_enabled  BOOLEAN         NOT NULL DEFAULT TRUE,
    deprovision_enabled BOOLEAN         NOT NULL DEFAULT TRUE,
    status              SMALLINT        NOT NULL DEFAULT 1,
    sync_cursor         VARCHAR(255)    NOT NULL DEFAULT '',
    last_sync_at        TIMESTAMPTZ,
    config              JSONB           NOT NULL DEFAULT '{}'::jsonb,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_org_scim_config_org_provider_code ON ai.org_scim_config (organization_id, provider_code);
CREATE INDEX idx_ai_org_scim_config_org_status ON ai.org_scim_config (organization_id, status);

COMMENT ON TABLE ai.org_scim_config IS 'AI 组织 SCIM 配置表';
COMMENT ON COLUMN ai.org_scim_config.id IS 'SCIM 配置ID';
COMMENT ON COLUMN ai.org_scim_config.organization_id IS '组织ID';
COMMENT ON COLUMN ai.org_scim_config.provider_code IS 'SCIM 提供方编码（组织内唯一）';
COMMENT ON COLUMN ai.org_scim_config.base_url IS 'SCIM 基础地址';
COMMENT ON COLUMN ai.org_scim_config.auth_type IS '鉴权方式：bearer/basic';
COMMENT ON COLUMN ai.org_scim_config.bearer_token_ref IS 'SCIM 访问令牌引用';
COMMENT ON COLUMN ai.org_scim_config.provisioning_mode IS '开通模式：push/pull/bidirectional';
COMMENT ON COLUMN ai.org_scim_config.sync_interval_minutes IS '同步间隔（分钟）';
COMMENT ON COLUMN ai.org_scim_config.user_sync_enabled IS '是否同步用户';
COMMENT ON COLUMN ai.org_scim_config.group_sync_enabled IS '是否同步组';
COMMENT ON COLUMN ai.org_scim_config.deprovision_enabled IS '是否启用停用/删除同步';
COMMENT ON COLUMN ai.org_scim_config.status IS '状态：1=启用 2=禁用 3=测试';
COMMENT ON COLUMN ai.org_scim_config.sync_cursor IS '同步游标/增量锚点';
COMMENT ON COLUMN ai.org_scim_config.last_sync_at IS '最后同步时间';
COMMENT ON COLUMN ai.org_scim_config.config IS '协议配置（JSON）';
COMMENT ON COLUMN ai.org_scim_config.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.org_scim_config.remark IS '备注';
COMMENT ON COLUMN ai.org_scim_config.create_by IS '创建人';
COMMENT ON COLUMN ai.org_scim_config.create_time IS '创建时间';
COMMENT ON COLUMN ai.org_scim_config.update_by IS '更新人';
COMMENT ON COLUMN ai.org_scim_config.update_time IS '更新时间';
