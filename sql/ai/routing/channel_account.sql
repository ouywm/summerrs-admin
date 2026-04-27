-- ============================================================
-- AI 渠道账号/密钥池表
-- 参考 sub2api accounts / APIKey 池、axonhub provider_quota_status
-- 一个渠道下可以挂多个实际可调度账号（api_key/oauth/cookie/session）
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.channel_account (
    id                  BIGSERIAL       PRIMARY KEY,
    channel_id          BIGINT          NOT NULL,
    name                VARCHAR(128)    NOT NULL DEFAULT '',
    credential_type     VARCHAR(32)     NOT NULL DEFAULT 'api_key',
    credentials         JSONB           NOT NULL DEFAULT '{}'::jsonb,
    secret_ref          VARCHAR(256)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    schedulable         BOOLEAN         NOT NULL DEFAULT TRUE,
    priority            INT             NOT NULL DEFAULT 0,
    weight              INT             NOT NULL DEFAULT 1 CHECK (weight >= 1),
    rate_multiplier     DECIMAL(10,4)   NOT NULL DEFAULT 1.0,
    concurrency_limit   INT             NOT NULL DEFAULT 0,
    quota_limit         DECIMAL(20,8)   NOT NULL DEFAULT 0,
    quota_used          DECIMAL(20,8)   NOT NULL DEFAULT 0,
    balance             DECIMAL(20,8)   NOT NULL DEFAULT 0,
    balance_updated_at  TIMESTAMPTZ,
    response_time       INT             NOT NULL DEFAULT 0,
    failure_streak      INT             NOT NULL DEFAULT 0,
    last_used_at        TIMESTAMPTZ,
    last_error_at       TIMESTAMPTZ,
    last_error_code     VARCHAR(64)     NOT NULL DEFAULT '',
    last_error_message  TEXT            NOT NULL DEFAULT '',
    rate_limited_until  TIMESTAMPTZ,
    overload_until      TIMESTAMPTZ,
    expires_at          TIMESTAMPTZ,
    test_model          VARCHAR(128)    NOT NULL DEFAULT '',
    test_time           TIMESTAMPTZ,
    extra               JSONB           NOT NULL DEFAULT '{}'::jsonb,
    deleted_at          TIMESTAMPTZ,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_channel_account_channel_name_active
    ON ai.channel_account (channel_id, name)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_ai_channel_account_channel_id ON ai.channel_account (channel_id);
CREATE INDEX idx_ai_channel_account_status_schedulable ON ai.channel_account (status, schedulable);
CREATE INDEX idx_ai_channel_account_priority_weight ON ai.channel_account (priority, weight);
CREATE INDEX idx_ai_channel_account_last_used_at ON ai.channel_account (last_used_at);
CREATE INDEX idx_ai_channel_account_rate_limited_until ON ai.channel_account (rate_limited_until);
CREATE INDEX idx_ai_channel_account_overload_until ON ai.channel_account (overload_until);

COMMENT ON TABLE ai.channel_account IS 'AI 渠道账号/密钥池表（一个渠道下的实际可调度账号）';
COMMENT ON COLUMN ai.channel_account.id IS '账号ID';
COMMENT ON COLUMN ai.channel_account.channel_id IS '所属渠道ID（ai.channel.id）';
COMMENT ON COLUMN ai.channel_account.name IS '账号名称（便于识别具体 Key/OAuth 账号）';
COMMENT ON COLUMN ai.channel_account.credential_type IS '凭证类型：api_key/oauth/cookie/session/token 等';
COMMENT ON COLUMN ai.channel_account.credentials IS '凭证载荷（JSON，如 {"api_key": "..."}、OAuth token、cookie 等）';
COMMENT ON COLUMN ai.channel_account.secret_ref IS '外部密钥管理引用（如 Vault/KMS 路径），为空表示直接落库 credentials';
COMMENT ON COLUMN ai.channel_account.status IS '状态：1=启用 2=禁用 3=额度耗尽 4=过期 5=冷却中';
COMMENT ON COLUMN ai.channel_account.schedulable IS '当前是否允许被路由器调度';
COMMENT ON COLUMN ai.channel_account.priority IS '账号优先级（同渠道内可二次调度）';
COMMENT ON COLUMN ai.channel_account.weight IS '账号权重（同优先级内加权随机）';
COMMENT ON COLUMN ai.channel_account.rate_multiplier IS '账号级成本倍率快照，可用于不同账号不同采购价';
COMMENT ON COLUMN ai.channel_account.concurrency_limit IS '并发上限（0=不限制）';
COMMENT ON COLUMN ai.channel_account.quota_limit IS '账号总额度上限（0=未知/不限制）';
COMMENT ON COLUMN ai.channel_account.quota_used IS '账号已用额度';
COMMENT ON COLUMN ai.channel_account.balance IS '账号级余额快照';
COMMENT ON COLUMN ai.channel_account.balance_updated_at IS '账号余额更新时间';
COMMENT ON COLUMN ai.channel_account.response_time IS '最近测速响应时间（毫秒）';
COMMENT ON COLUMN ai.channel_account.failure_streak IS '连续失败次数';
COMMENT ON COLUMN ai.channel_account.last_used_at IS '最近一次实际使用时间';
COMMENT ON COLUMN ai.channel_account.last_error_at IS '最近错误时间';
COMMENT ON COLUMN ai.channel_account.last_error_code IS '最近错误码';
COMMENT ON COLUMN ai.channel_account.last_error_message IS '最近错误摘要';
COMMENT ON COLUMN ai.channel_account.rate_limited_until IS '速率限制冷却到期时间';
COMMENT ON COLUMN ai.channel_account.overload_until IS '上游过载冷却到期时间';
COMMENT ON COLUMN ai.channel_account.expires_at IS '账号凭证失效时间';
COMMENT ON COLUMN ai.channel_account.test_model IS '账号级测速模型';
COMMENT ON COLUMN ai.channel_account.test_time IS '最近测速时间';
COMMENT ON COLUMN ai.channel_account.extra IS '账号级扩展字段（JSON）';
COMMENT ON COLUMN ai.channel_account.deleted_at IS '软删除时间';
COMMENT ON COLUMN ai.channel_account.remark IS '备注';
COMMENT ON COLUMN ai.channel_account.create_by IS '创建人';
COMMENT ON COLUMN ai.channel_account.create_time IS '创建时间';
COMMENT ON COLUMN ai.channel_account.update_by IS '更新人';
COMMENT ON COLUMN ai.channel_account.update_time IS '更新时间';
