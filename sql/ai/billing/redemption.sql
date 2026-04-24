-- ============================================================
-- AI 兑换码表
-- 在 one-api redemption 基础上增加有效期和目标分组字段
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.redemption (
    id                BIGSERIAL       PRIMARY KEY,
    name              VARCHAR(128)    NOT NULL DEFAULT '',
    code              VARCHAR(64)     NOT NULL,
    quota             BIGINT          NOT NULL DEFAULT 0,
    allow_group_code  VARCHAR(64)     NOT NULL DEFAULT '',
    status            SMALLINT        NOT NULL DEFAULT 1,
    count             INT             NOT NULL DEFAULT 1,
    used_count        INT             NOT NULL DEFAULT 0,
    redeemed_user_id  BIGINT,
    expire_time       TIMESTAMPTZ,
    redeem_time       TIMESTAMPTZ,
    remark            VARCHAR(500)    NOT NULL DEFAULT '',
    create_by         VARCHAR(64)     NOT NULL DEFAULT '',
    create_time       TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by         VARCHAR(64)     NOT NULL DEFAULT '',
    update_time       TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_redemption_code ON ai.redemption (code);
CREATE INDEX idx_ai_redemption_status ON ai.redemption (status);
CREATE INDEX idx_ai_redemption_expire_time ON ai.redemption (expire_time);

COMMENT ON TABLE ai.redemption IS 'AI 兑换码表（可按有效期和目标分组发放额度）';
COMMENT ON COLUMN ai.redemption.id IS '兑换码ID';
COMMENT ON COLUMN ai.redemption.name IS '兑换码名称/批次备注';
COMMENT ON COLUMN ai.redemption.code IS '兑换码';
COMMENT ON COLUMN ai.redemption.quota IS '兑换额度';
COMMENT ON COLUMN ai.redemption.allow_group_code IS '兑换后切换/授予的目标分组（空=不变）';
COMMENT ON COLUMN ai.redemption.status IS '状态：1=未使用 2=已禁用 3=已使用/已发完';
COMMENT ON COLUMN ai.redemption.count IS '可兑换次数';
COMMENT ON COLUMN ai.redemption.used_count IS '已兑换次数';
COMMENT ON COLUMN ai.redemption.redeemed_user_id IS '最后兑换者用户ID';
COMMENT ON COLUMN ai.redemption.expire_time IS '过期时间';
COMMENT ON COLUMN ai.redemption.redeem_time IS '最后一次兑换时间';
COMMENT ON COLUMN ai.redemption.remark IS '备注';
COMMENT ON COLUMN ai.redemption.create_by IS '创建人';
COMMENT ON COLUMN ai.redemption.create_time IS '创建时间';
COMMENT ON COLUMN ai.redemption.update_by IS '更新人';
COMMENT ON COLUMN ai.redemption.update_time IS '更新时间';
