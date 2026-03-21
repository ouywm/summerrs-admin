-- ============================================================
-- AI 令牌表
-- 对标 one-api Token / Hadrian api_keys，但增强为哈希存储 + 限流/预算控制
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.token (
    id                  BIGSERIAL       PRIMARY KEY,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    service_account_id  BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    name                VARCHAR(128)    NOT NULL DEFAULT '',
    key_hash            VARCHAR(64)     NOT NULL,
    key_prefix          VARCHAR(16)     NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    remain_quota        BIGINT          NOT NULL DEFAULT 0,
    used_quota          BIGINT          NOT NULL DEFAULT 0,
    unlimited_quota     BOOLEAN         NOT NULL DEFAULT FALSE,
    models              JSONB           NOT NULL DEFAULT '[]'::jsonb,
    endpoint_scopes     JSONB           NOT NULL DEFAULT '[]'::jsonb,
    ip_whitelist        JSONB           NOT NULL DEFAULT '[]'::jsonb,
    ip_blacklist        JSONB           NOT NULL DEFAULT '[]'::jsonb,
    group_code_override VARCHAR(64)     NOT NULL DEFAULT '',
    rpm_limit           INT             NOT NULL DEFAULT 0,
    tpm_limit           BIGINT          NOT NULL DEFAULT 0,
    concurrency_limit   INT             NOT NULL DEFAULT 0,
    daily_quota_limit   BIGINT          NOT NULL DEFAULT 0,
    monthly_quota_limit BIGINT          NOT NULL DEFAULT 0,
    daily_used_quota    BIGINT          NOT NULL DEFAULT 0,
    monthly_used_quota  BIGINT          NOT NULL DEFAULT 0,
    daily_window_start  TIMESTAMPTZ,
    monthly_window_start TIMESTAMPTZ,
    expire_time         TIMESTAMPTZ,
    access_time         TIMESTAMPTZ,
    last_used_ip        VARCHAR(64)     NOT NULL DEFAULT '',
    last_user_agent     VARCHAR(512)    NOT NULL DEFAULT '',
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_token_key_hash ON ai.token (key_hash);
CREATE INDEX idx_ai_token_user_id ON ai.token (user_id);
CREATE INDEX idx_ai_token_service_account_id ON ai.token (service_account_id);
CREATE INDEX idx_ai_token_project_id ON ai.token (project_id);
CREATE INDEX idx_ai_token_status ON ai.token (status);
CREATE INDEX idx_ai_token_group_code_override ON ai.token (group_code_override);
CREATE INDEX idx_ai_token_expire_time ON ai.token (expire_time);

COMMENT ON TABLE ai.token IS 'AI 令牌表（下游调用方 API Key，明文只展示一次，库中仅存哈希）';
COMMENT ON COLUMN ai.token.id IS '令牌ID';
COMMENT ON COLUMN ai.token.user_id IS '所属用户ID（关联 sys."user".id；个人令牌时为拥有者，服务账号令牌时通常为创建者）';
COMMENT ON COLUMN ai.token.service_account_id IS '绑定的服务账号ID（0 表示用户个人令牌）';
COMMENT ON COLUMN ai.token.project_id IS '所属项目ID（0 表示不绑定项目/仅绑定组织或服务账号）';
COMMENT ON COLUMN ai.token.name IS '令牌名称（便于识别用途）';
COMMENT ON COLUMN ai.token.key_hash IS 'API Key 的 SHA-256 哈希值';
COMMENT ON COLUMN ai.token.key_prefix IS 'API Key 前缀（如 sk-aBcD，用于 UI 展示）';
COMMENT ON COLUMN ai.token.status IS '状态：1=启用 2=禁用 3=已过期 4=额度耗尽';
COMMENT ON COLUMN ai.token.remain_quota IS '剩余配额';
COMMENT ON COLUMN ai.token.used_quota IS '累计已用配额';
COMMENT ON COLUMN ai.token.unlimited_quota IS '是否不限额度';
COMMENT ON COLUMN ai.token.models IS '允许使用的模型白名单（JSON 数组，空数组=不限制）';
COMMENT ON COLUMN ai.token.endpoint_scopes IS '允许使用的 endpoint 白名单（JSON 数组，空数组=不限制）';
COMMENT ON COLUMN ai.token.ip_whitelist IS 'IP 白名单（JSON 数组，支持 IP/CIDR）';
COMMENT ON COLUMN ai.token.ip_blacklist IS 'IP 黑名单（JSON 数组，支持 IP/CIDR）';
COMMENT ON COLUMN ai.token.group_code_override IS '令牌级分组覆盖（为空则跟随 ai.user_quota.channel_group）';
COMMENT ON COLUMN ai.token.rpm_limit IS '每分钟请求数限制（0=不限制）';
COMMENT ON COLUMN ai.token.tpm_limit IS '每分钟 token 数限制（0=不限制）';
COMMENT ON COLUMN ai.token.concurrency_limit IS '并发限制（0=不限制）';
COMMENT ON COLUMN ai.token.daily_quota_limit IS '日额度上限（0=不限制）';
COMMENT ON COLUMN ai.token.monthly_quota_limit IS '月额度上限（0=不限制）';
COMMENT ON COLUMN ai.token.daily_used_quota IS '当前日窗口已用额度';
COMMENT ON COLUMN ai.token.monthly_used_quota IS '当前月窗口已用额度';
COMMENT ON COLUMN ai.token.daily_window_start IS '当前日窗口起始时间';
COMMENT ON COLUMN ai.token.monthly_window_start IS '当前月窗口起始时间';
COMMENT ON COLUMN ai.token.expire_time IS '过期时间（NULL=永不过期）';
COMMENT ON COLUMN ai.token.access_time IS '最后访问时间';
COMMENT ON COLUMN ai.token.last_used_ip IS '最近访问 IP';
COMMENT ON COLUMN ai.token.last_user_agent IS '最近访问 UA';
COMMENT ON COLUMN ai.token.remark IS '备注';
COMMENT ON COLUMN ai.token.create_by IS '创建人';
COMMENT ON COLUMN ai.token.create_time IS '创建时间';
COMMENT ON COLUMN ai.token.update_by IS '更新人';
COMMENT ON COLUMN ai.token.update_time IS '更新时间';
