-- ============================================================
-- AI 用户配额表
-- 对标 one-api/new-api User.quota，但增强了日/月预算窗口与状态控制
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.user_quota (
    id                  BIGSERIAL       PRIMARY KEY,
    user_id             BIGINT          NOT NULL,
    channel_group       VARCHAR(64)     NOT NULL DEFAULT 'default',
    status              SMALLINT        NOT NULL DEFAULT 1,
    quota               BIGINT          NOT NULL DEFAULT 0,
    used_quota          BIGINT          NOT NULL DEFAULT 0,
    request_count       BIGINT          NOT NULL DEFAULT 0,
    daily_quota_limit   BIGINT          NOT NULL DEFAULT 0,
    monthly_quota_limit BIGINT          NOT NULL DEFAULT 0,
    daily_used_quota    BIGINT          NOT NULL DEFAULT 0,
    monthly_used_quota  BIGINT          NOT NULL DEFAULT 0,
    daily_window_start  TIMESTAMPTZ,
    monthly_window_start TIMESTAMPTZ,
    last_request_time   TIMESTAMPTZ,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_user_quota_user_id ON ai.user_quota (user_id);
CREATE INDEX idx_ai_user_quota_channel_group ON ai.user_quota (channel_group);
CREATE INDEX idx_ai_user_quota_status ON ai.user_quota (status);

COMMENT ON TABLE ai.user_quota IS 'AI 用户配额表（用户在 AI 网关中的额度与预算窗口）';
COMMENT ON COLUMN ai.user_quota.id IS '配额ID';
COMMENT ON COLUMN ai.user_quota.user_id IS '用户ID（关联 sys."user".id，唯一）';
COMMENT ON COLUMN ai.user_quota.channel_group IS '所属分组（决定可用渠道和计费倍率）';
COMMENT ON COLUMN ai.user_quota.status IS '状态：1=正常 2=禁用 3=冻结';
COMMENT ON COLUMN ai.user_quota.quota IS '总配额（累计授予额度）';
COMMENT ON COLUMN ai.user_quota.used_quota IS '累计已消耗配额';
COMMENT ON COLUMN ai.user_quota.request_count IS '累计请求次数';
COMMENT ON COLUMN ai.user_quota.daily_quota_limit IS '日额度上限（0=不限制）';
COMMENT ON COLUMN ai.user_quota.monthly_quota_limit IS '月额度上限（0=不限制）';
COMMENT ON COLUMN ai.user_quota.daily_used_quota IS '当前日窗口已用额度';
COMMENT ON COLUMN ai.user_quota.monthly_used_quota IS '当前月窗口已用额度';
COMMENT ON COLUMN ai.user_quota.daily_window_start IS '当前日窗口起始时间';
COMMENT ON COLUMN ai.user_quota.monthly_window_start IS '当前月窗口起始时间';
COMMENT ON COLUMN ai.user_quota.last_request_time IS '最后一次请求时间';
COMMENT ON COLUMN ai.user_quota.remark IS '备注';
COMMENT ON COLUMN ai.user_quota.create_by IS '创建人';
COMMENT ON COLUMN ai.user_quota.create_time IS '创建时间';
COMMENT ON COLUMN ai.user_quota.update_by IS '更新人';
COMMENT ON COLUMN ai.user_quota.update_time IS '更新时间';
