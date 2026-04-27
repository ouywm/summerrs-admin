-- ============================================================
-- AI 充值/支付流水表
-- 兼容钱包充值，也可承载订阅套餐支付流水
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.topup (
    id                  BIGSERIAL       PRIMARY KEY,
    user_id             BIGINT          NOT NULL,
    trade_no            VARCHAR(64)     NOT NULL,
    subscription_plan_id BIGINT         NOT NULL DEFAULT 0,
    amount              BIGINT          NOT NULL DEFAULT 0,
    money               DECIMAL(12,2)   NOT NULL DEFAULT 0,
    currency            VARCHAR(8)      NOT NULL DEFAULT 'CNY',
    payment_method      VARCHAR(32)     NOT NULL DEFAULT '',
    topup_type          SMALLINT        NOT NULL DEFAULT 1,
    status              SMALLINT        NOT NULL DEFAULT 1,
    payment_payload     JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    complete_time       TIMESTAMPTZ,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_topup_trade_no ON ai.topup (trade_no);
CREATE INDEX idx_ai_topup_user_id ON ai.topup (user_id);
CREATE INDEX idx_ai_topup_subscription_plan_id ON ai.topup (subscription_plan_id);
CREATE INDEX idx_ai_topup_status ON ai.topup (status);
CREATE INDEX idx_ai_topup_create_time ON ai.topup (create_time);

COMMENT ON TABLE ai.topup IS 'AI 充值/支付流水表（钱包充值与订阅支付共用）';
COMMENT ON COLUMN ai.topup.id IS '流水ID';
COMMENT ON COLUMN ai.topup.user_id IS '用户ID';
COMMENT ON COLUMN ai.topup.trade_no IS '交易单号（唯一）';
COMMENT ON COLUMN ai.topup.subscription_plan_id IS '订阅套餐ID（0=普通充值）';
COMMENT ON COLUMN ai.topup.amount IS '充值额度或订阅授予额度';
COMMENT ON COLUMN ai.topup.money IS '支付金额';
COMMENT ON COLUMN ai.topup.currency IS '货币代码';
COMMENT ON COLUMN ai.topup.payment_method IS '支付方式（alipay/wechat/stripe/admin_grant/redemption 等）';
COMMENT ON COLUMN ai.topup.topup_type IS '类型：1=在线支付 2=管理员充值 3=兑换码 4=系统赠送 5=订阅购买';
COMMENT ON COLUMN ai.topup.status IS '状态：1=待支付 2=已完成 3=已取消 4=已退款';
COMMENT ON COLUMN ai.topup.payment_payload IS '支付网关原始载荷（JSON）';
COMMENT ON COLUMN ai.topup.remark IS '备注';
COMMENT ON COLUMN ai.topup.complete_time IS '完成时间';
COMMENT ON COLUMN ai.topup.create_by IS '创建人';
COMMENT ON COLUMN ai.topup.create_time IS '创建时间';
COMMENT ON COLUMN ai.topup.update_by IS '更新人';
COMMENT ON COLUMN ai.topup.update_time IS '更新时间';
