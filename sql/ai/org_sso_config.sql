-- ============================================================
-- AI 组织 SSO 配置表
-- 参考 hadrian org_sso_configs / 企业身份接入层
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.org_sso_config (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL REFERENCES ai.organization(id) ON DELETE CASCADE,
    provider_code       VARCHAR(64)     NOT NULL DEFAULT 'default',
    provider_name       VARCHAR(128)    NOT NULL DEFAULT '',
    protocol_type       VARCHAR(32)     NOT NULL DEFAULT 'oidc',
    issuer              VARCHAR(255)    NOT NULL DEFAULT '',
    entrypoint_url      VARCHAR(512)    NOT NULL DEFAULT '',
    callback_url        VARCHAR(512)    NOT NULL DEFAULT '',
    entity_id           VARCHAR(255)    NOT NULL DEFAULT '',
    audience            VARCHAR(255)    NOT NULL DEFAULT '',
    client_id           VARCHAR(255)    NOT NULL DEFAULT '',
    client_secret_ref   VARCHAR(255)    NOT NULL DEFAULT '',
    certificate_pem     TEXT            NOT NULL DEFAULT '',
    allowed_domains     JSONB           NOT NULL DEFAULT '[]'::jsonb,
    default_role_code   VARCHAR(64)     NOT NULL DEFAULT '',
    jit_enabled         BOOLEAN         NOT NULL DEFAULT TRUE,
    auto_provision      BOOLEAN         NOT NULL DEFAULT TRUE,
    is_default          BOOLEAN         NOT NULL DEFAULT FALSE,
    status              SMALLINT        NOT NULL DEFAULT 1,
    last_used_at        TIMESTAMPTZ,
    config              JSONB           NOT NULL DEFAULT '{}'::jsonb,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_org_sso_config_org_provider_code ON ai.org_sso_config (organization_id, provider_code);
CREATE INDEX idx_ai_org_sso_config_org_status ON ai.org_sso_config (organization_id, status);
CREATE INDEX idx_ai_org_sso_config_protocol_type ON ai.org_sso_config (protocol_type);

COMMENT ON TABLE ai.org_sso_config IS 'AI 组织 SSO 配置表';
COMMENT ON COLUMN ai.org_sso_config.id IS 'SSO 配置ID';
COMMENT ON COLUMN ai.org_sso_config.organization_id IS '组织ID';
COMMENT ON COLUMN ai.org_sso_config.provider_code IS 'SSO 提供方编码（组织内唯一）';
COMMENT ON COLUMN ai.org_sso_config.provider_name IS 'SSO 提供方名称';
COMMENT ON COLUMN ai.org_sso_config.protocol_type IS '协议类型：oidc/saml';
COMMENT ON COLUMN ai.org_sso_config.issuer IS 'OIDC/SAML issuer';
COMMENT ON COLUMN ai.org_sso_config.entrypoint_url IS '登录入口地址';
COMMENT ON COLUMN ai.org_sso_config.callback_url IS '回调地址';
COMMENT ON COLUMN ai.org_sso_config.entity_id IS 'SAML Entity ID 或应用标识';
COMMENT ON COLUMN ai.org_sso_config.audience IS 'Audience';
COMMENT ON COLUMN ai.org_sso_config.client_id IS '客户端 ID';
COMMENT ON COLUMN ai.org_sso_config.client_secret_ref IS '客户端密钥引用';
COMMENT ON COLUMN ai.org_sso_config.certificate_pem IS '证书内容';
COMMENT ON COLUMN ai.org_sso_config.allowed_domains IS '允许自动接入的域名列表（JSON 数组）';
COMMENT ON COLUMN ai.org_sso_config.default_role_code IS '自动开通时默认角色';
COMMENT ON COLUMN ai.org_sso_config.jit_enabled IS '是否启用 JIT 登录开通';
COMMENT ON COLUMN ai.org_sso_config.auto_provision IS '是否自动创建/同步成员';
COMMENT ON COLUMN ai.org_sso_config.is_default IS '是否默认 SSO 配置';
COMMENT ON COLUMN ai.org_sso_config.status IS '状态：1=启用 2=禁用 3=测试';
COMMENT ON COLUMN ai.org_sso_config.last_used_at IS '最近使用时间';
COMMENT ON COLUMN ai.org_sso_config.config IS '协议配置（JSON）';
COMMENT ON COLUMN ai.org_sso_config.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.org_sso_config.remark IS '备注';
COMMENT ON COLUMN ai.org_sso_config.create_by IS '创建人';
COMMENT ON COLUMN ai.org_sso_config.create_time IS '创建时间';
COMMENT ON COLUMN ai.org_sso_config.update_by IS '更新人';
COMMENT ON COLUMN ai.org_sso_config.update_time IS '更新时间';
