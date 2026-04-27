-- ============================================================
-- AI 邀请返利表
-- 参考 llmgateway referral / 用户与组织拉新奖励
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.referral (
    id                  BIGSERIAL       PRIMARY KEY,
    referrer_user_id    BIGINT          NOT NULL DEFAULT 0,
    referrer_org_id     BIGINT          NOT NULL DEFAULT 0,
    referred_user_id    BIGINT          NOT NULL DEFAULT 0,
    referred_org_id     BIGINT          NOT NULL DEFAULT 0,
    invite_code         VARCHAR(64)     NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    reward_type         VARCHAR(32)     NOT NULL DEFAULT 'quota',
    reward_amount       DECIMAL(20,8)   NOT NULL DEFAULT 0,
    reward_quota        BIGINT          NOT NULL DEFAULT 0,
    reward_currency     VARCHAR(8)      NOT NULL DEFAULT 'USD',
    settled_time        TIMESTAMPTZ,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_referral_referrer_user_id ON ai.referral (referrer_user_id);
CREATE INDEX idx_ai_referral_referred_user_id ON ai.referral (referred_user_id);
CREATE INDEX idx_ai_referral_referrer_org_id ON ai.referral (referrer_org_id);
CREATE INDEX idx_ai_referral_status ON ai.referral (status);
CREATE INDEX idx_ai_referral_invite_code ON ai.referral (invite_code);

COMMENT ON TABLE ai.referral IS 'AI 邀请返利表';
COMMENT ON COLUMN ai.referral.id IS '返利记录ID';
COMMENT ON COLUMN ai.referral.referrer_user_id IS '邀请人用户ID';
COMMENT ON COLUMN ai.referral.referrer_org_id IS '邀请人组织ID';
COMMENT ON COLUMN ai.referral.referred_user_id IS '被邀请用户ID';
COMMENT ON COLUMN ai.referral.referred_org_id IS '被邀请组织ID';
COMMENT ON COLUMN ai.referral.invite_code IS '邀请码';
COMMENT ON COLUMN ai.referral.status IS '状态：1=待结算 2=已结算 3=失效';
COMMENT ON COLUMN ai.referral.reward_type IS '奖励类型：quota/cash/credit';
COMMENT ON COLUMN ai.referral.reward_amount IS '奖励金额';
COMMENT ON COLUMN ai.referral.reward_quota IS '奖励额度';
COMMENT ON COLUMN ai.referral.reward_currency IS '奖励货币';
COMMENT ON COLUMN ai.referral.settled_time IS '结算时间';
COMMENT ON COLUMN ai.referral.metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai.referral.create_time IS '创建时间';
COMMENT ON COLUMN ai.referral.update_time IS '更新时间';
