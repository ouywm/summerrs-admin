-- ============================================================
-- 系统自定义 OAuth 提供方表
-- 用于后台配置 GitHub/Google/飞书/企业微信/OIDC 等登录方式
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.custom_oauth_provider (
    id                 BIGSERIAL       PRIMARY KEY,
    provider_code      VARCHAR(64)     NOT NULL,
    provider_name      VARCHAR(128)    NOT NULL DEFAULT '',
    provider_type      VARCHAR(32)     NOT NULL DEFAULT 'oidc',
    client_id          VARCHAR(255)    NOT NULL DEFAULT '',
    client_secret_ref  VARCHAR(255)    NOT NULL DEFAULT '',
    authorize_url      VARCHAR(512)    NOT NULL DEFAULT '',
    token_url          VARCHAR(512)    NOT NULL DEFAULT '',
    userinfo_url       VARCHAR(512)    NOT NULL DEFAULT '',
    jwks_url           VARCHAR(512)    NOT NULL DEFAULT '',
    issuer             VARCHAR(255)    NOT NULL DEFAULT '',
    scopes             JSONB           NOT NULL DEFAULT '[]'::jsonb,
    pkce_enabled       BOOLEAN         NOT NULL DEFAULT TRUE,
    auto_create_user   BOOLEAN         NOT NULL DEFAULT TRUE,
    auto_link_by_email BOOLEAN         NOT NULL DEFAULT FALSE,
    icon_url           VARCHAR(512)    NOT NULL DEFAULT '',
    sort               INT             NOT NULL DEFAULT 0,
    status             SMALLINT        NOT NULL DEFAULT 1,
    claim_mapping      JSONB           NOT NULL DEFAULT '{}'::jsonb,
    extra_config       JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_by          VARCHAR(64)     NOT NULL DEFAULT '',
    create_time        TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by          VARCHAR(64)     NOT NULL DEFAULT '',
    update_time        TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_custom_oauth_provider_code ON sys.custom_oauth_provider (provider_code);
CREATE INDEX idx_sys_custom_oauth_provider_status_sort ON sys.custom_oauth_provider (status, sort);
CREATE INDEX idx_sys_custom_oauth_provider_type ON sys.custom_oauth_provider (provider_type);

COMMENT ON TABLE sys.custom_oauth_provider IS '系统自定义 OAuth 提供方表';
COMMENT ON COLUMN sys.custom_oauth_provider.id IS '主键ID';
COMMENT ON COLUMN sys.custom_oauth_provider.provider_code IS '提供方编码';
COMMENT ON COLUMN sys.custom_oauth_provider.provider_name IS '提供方名称';
COMMENT ON COLUMN sys.custom_oauth_provider.provider_type IS '提供方类型：github/google/feishu/dingtalk/wecom/oidc';
COMMENT ON COLUMN sys.custom_oauth_provider.client_id IS '客户端ID';
COMMENT ON COLUMN sys.custom_oauth_provider.client_secret_ref IS '客户端密钥引用';
COMMENT ON COLUMN sys.custom_oauth_provider.authorize_url IS '授权地址';
COMMENT ON COLUMN sys.custom_oauth_provider.token_url IS '令牌地址';
COMMENT ON COLUMN sys.custom_oauth_provider.userinfo_url IS '用户信息地址';
COMMENT ON COLUMN sys.custom_oauth_provider.jwks_url IS 'JWKS 地址';
COMMENT ON COLUMN sys.custom_oauth_provider.issuer IS 'Issuer';
COMMENT ON COLUMN sys.custom_oauth_provider.scopes IS '授权范围（JSON 数组）';
COMMENT ON COLUMN sys.custom_oauth_provider.pkce_enabled IS '是否启用 PKCE';
COMMENT ON COLUMN sys.custom_oauth_provider.auto_create_user IS '首次登录是否自动创建用户';
COMMENT ON COLUMN sys.custom_oauth_provider.auto_link_by_email IS '是否按邮箱自动绑定已有用户';
COMMENT ON COLUMN sys.custom_oauth_provider.icon_url IS '图标地址';
COMMENT ON COLUMN sys.custom_oauth_provider.sort IS '排序';
COMMENT ON COLUMN sys.custom_oauth_provider.status IS '状态：1=启用 2=禁用 3=测试';
COMMENT ON COLUMN sys.custom_oauth_provider.claim_mapping IS 'Claim 映射配置（JSON）';
COMMENT ON COLUMN sys.custom_oauth_provider.extra_config IS '扩展配置（JSON）';
COMMENT ON COLUMN sys.custom_oauth_provider.create_by IS '创建人';
COMMENT ON COLUMN sys.custom_oauth_provider.create_time IS '创建时间';
COMMENT ON COLUMN sys.custom_oauth_provider.update_by IS '更新人';
COMMENT ON COLUMN sys.custom_oauth_provider.update_time IS '更新时间';
