-- ============================================================
-- AI 组织表
-- 参考 hadrian organizations / llmgateway organization
-- 适用于团队协作、多项目、多租户控制面
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.organization (
    id                  BIGSERIAL       PRIMARY KEY,
    org_code            VARCHAR(64)     NOT NULL,
    org_name            VARCHAR(128)    NOT NULL DEFAULT '',
    owner_user_id       BIGINT          NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    default_group_code  VARCHAR(64)     NOT NULL DEFAULT '',
    billing_email       VARCHAR(255)    NOT NULL DEFAULT '',
    billing_mode        VARCHAR(32)     NOT NULL DEFAULT 'wallet',
    quota_limit         BIGINT          NOT NULL DEFAULT 0,
    used_quota          BIGINT          NOT NULL DEFAULT 0,
    daily_quota_limit   BIGINT          NOT NULL DEFAULT 0,
    monthly_quota_limit BIGINT          NOT NULL DEFAULT 0,
    request_count       BIGINT          NOT NULL DEFAULT 0,
    settings            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_organization_org_code ON ai.organization (org_code);
CREATE INDEX idx_ai_organization_owner_user_id ON ai.organization (owner_user_id);
CREATE INDEX idx_ai_organization_status ON ai.organization (status);
CREATE INDEX idx_ai_organization_billing_mode ON ai.organization (billing_mode);

COMMENT ON TABLE ai.organization IS 'AI 组织表（多租户组织根实体）';
COMMENT ON COLUMN ai.organization.id IS '组织ID';
COMMENT ON COLUMN ai.organization.org_code IS '组织编码（唯一）';
COMMENT ON COLUMN ai.organization.org_name IS '组织名称';
COMMENT ON COLUMN ai.organization.owner_user_id IS '组织拥有者用户ID';
COMMENT ON COLUMN ai.organization.status IS '状态：1=启用 2=禁用 3=归档';
COMMENT ON COLUMN ai.organization.default_group_code IS '默认用户分组';
COMMENT ON COLUMN ai.organization.billing_email IS '账单通知邮箱';
COMMENT ON COLUMN ai.organization.billing_mode IS '计费模式：wallet/subscription/hybrid';
COMMENT ON COLUMN ai.organization.quota_limit IS '组织总额度上限（0=不限制）';
COMMENT ON COLUMN ai.organization.used_quota IS '组织累计已用额度';
COMMENT ON COLUMN ai.organization.daily_quota_limit IS '组织日额度上限';
COMMENT ON COLUMN ai.organization.monthly_quota_limit IS '组织月额度上限';
COMMENT ON COLUMN ai.organization.request_count IS '组织累计请求数';
COMMENT ON COLUMN ai.organization.settings IS '组织级设置（JSON）';
COMMENT ON COLUMN ai.organization.remark IS '备注';
COMMENT ON COLUMN ai.organization.create_by IS '创建人';
COMMENT ON COLUMN ai.organization.create_time IS '创建时间';
COMMENT ON COLUMN ai.organization.update_by IS '更新人';
COMMENT ON COLUMN ai.organization.update_time IS '更新时间';
