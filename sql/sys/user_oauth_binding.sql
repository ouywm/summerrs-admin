-- ============================================================
-- 系统用户 OAuth 绑定表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.user_oauth_binding (
    id                 BIGSERIAL       PRIMARY KEY,
    user_id            BIGINT          NOT NULL,
    provider_id        BIGINT          NOT NULL,
    external_user_id   VARCHAR(128)    NOT NULL,
    external_user_name VARCHAR(128)    NOT NULL DEFAULT '',
    external_email     VARCHAR(255)    NOT NULL DEFAULT '',
    access_token_ref   VARCHAR(255)    NOT NULL DEFAULT '',
    refresh_token_ref  VARCHAR(255)    NOT NULL DEFAULT '',
    expire_time        TIMESTAMP,
    last_login_time    TIMESTAMP,
    last_sync_time     TIMESTAMP,
    status             SMALLINT        NOT NULL DEFAULT 1,
    profile_payload    JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time        TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time        TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_user_oauth_binding_user_provider ON sys.user_oauth_binding (user_id, provider_id);
CREATE UNIQUE INDEX uk_sys_user_oauth_binding_provider_external_user ON sys.user_oauth_binding (provider_id, external_user_id);
CREATE INDEX idx_sys_user_oauth_binding_external_email ON sys.user_oauth_binding (external_email);
CREATE INDEX idx_sys_user_oauth_binding_status ON sys.user_oauth_binding (status);

COMMENT ON TABLE sys.user_oauth_binding IS '系统用户 OAuth 绑定表';
COMMENT ON COLUMN sys.user_oauth_binding.id IS '主键ID';
COMMENT ON COLUMN sys.user_oauth_binding.user_id IS '系统用户ID';
COMMENT ON COLUMN sys.user_oauth_binding.provider_id IS 'OAuth 提供方ID';
COMMENT ON COLUMN sys.user_oauth_binding.external_user_id IS '外部平台用户ID';
COMMENT ON COLUMN sys.user_oauth_binding.external_user_name IS '外部平台用户名';
COMMENT ON COLUMN sys.user_oauth_binding.external_email IS '外部平台邮箱';
COMMENT ON COLUMN sys.user_oauth_binding.access_token_ref IS '访问令牌引用';
COMMENT ON COLUMN sys.user_oauth_binding.refresh_token_ref IS '刷新令牌引用';
COMMENT ON COLUMN sys.user_oauth_binding.expire_time IS '令牌到期时间';
COMMENT ON COLUMN sys.user_oauth_binding.last_login_time IS '最近一次 OAuth 登录时间';
COMMENT ON COLUMN sys.user_oauth_binding.last_sync_time IS '最近一次资料同步时间';
COMMENT ON COLUMN sys.user_oauth_binding.status IS '状态：1=正常 2=停用 3=解绑';
COMMENT ON COLUMN sys.user_oauth_binding.profile_payload IS '外部资料快照（JSON）';
COMMENT ON COLUMN sys.user_oauth_binding.create_time IS '创建时间';
COMMENT ON COLUMN sys.user_oauth_binding.update_time IS '更新时间';
