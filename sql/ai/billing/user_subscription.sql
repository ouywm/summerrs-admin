-- ============================================================
-- AI 用户订阅表
-- 用户购买/分配后的实际订阅实例
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.user_subscription (
    id                  BIGSERIAL       PRIMARY KEY,
    user_id             BIGINT          NOT NULL,
    plan_id             BIGINT          NOT NULL,
    status              SMALLINT        NOT NULL DEFAULT 1,
    quota_total         BIGINT          NOT NULL DEFAULT 0,
    quota_used          BIGINT          NOT NULL DEFAULT 0,
    daily_used_quota    BIGINT          NOT NULL DEFAULT 0,
    monthly_used_quota  BIGINT          NOT NULL DEFAULT 0,
    start_time          TIMESTAMPTZ     NOT NULL,
    expire_time         TIMESTAMPTZ     NOT NULL,
    last_reset_time     TIMESTAMPTZ,
    next_reset_time     TIMESTAMPTZ,
    group_code_snapshot VARCHAR(64)     NOT NULL DEFAULT '',
    source_trade_no     VARCHAR(64)     NOT NULL DEFAULT '',
    assigned_by         BIGINT          NOT NULL DEFAULT 0,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_user_subscription_user_status_expire
    ON ai.user_subscription (user_id, status, expire_time);
CREATE INDEX idx_ai_user_subscription_plan_id ON ai.user_subscription (plan_id);
CREATE INDEX idx_ai_user_subscription_next_reset_time ON ai.user_subscription (next_reset_time);
CREATE INDEX idx_ai_user_subscription_trade_no ON ai.user_subscription (source_trade_no);

COMMENT ON TABLE ai.user_subscription IS 'AI 用户订阅表（用户实际拥有的订阅实例）';
COMMENT ON COLUMN ai.user_subscription.id IS '用户订阅ID';
COMMENT ON COLUMN ai.user_subscription.user_id IS '用户ID';
COMMENT ON COLUMN ai.user_subscription.plan_id IS '套餐ID';
COMMENT ON COLUMN ai.user_subscription.status IS '状态：1=生效中 2=已过期 3=已取消 4=额度耗尽';
COMMENT ON COLUMN ai.user_subscription.quota_total IS '订阅总额度';
COMMENT ON COLUMN ai.user_subscription.quota_used IS '订阅已用额度';
COMMENT ON COLUMN ai.user_subscription.daily_used_quota IS '当前日窗口已用额度';
COMMENT ON COLUMN ai.user_subscription.monthly_used_quota IS '当前月窗口已用额度';
COMMENT ON COLUMN ai.user_subscription.start_time IS '生效开始时间';
COMMENT ON COLUMN ai.user_subscription.expire_time IS '到期时间';
COMMENT ON COLUMN ai.user_subscription.last_reset_time IS '上次额度重置时间';
COMMENT ON COLUMN ai.user_subscription.next_reset_time IS '下次额度重置时间';
COMMENT ON COLUMN ai.user_subscription.group_code_snapshot IS '订阅生效时的分组快照';
COMMENT ON COLUMN ai.user_subscription.source_trade_no IS '来源交易单号';
COMMENT ON COLUMN ai.user_subscription.assigned_by IS '分配人ID（管理员分配时使用）';
COMMENT ON COLUMN ai.user_subscription.remark IS '备注';
COMMENT ON COLUMN ai.user_subscription.create_by IS '创建人';
COMMENT ON COLUMN ai.user_subscription.create_time IS '创建时间';
COMMENT ON COLUMN ai.user_subscription.update_by IS '更新人';
COMMENT ON COLUMN ai.user_subscription.update_time IS '更新时间';
