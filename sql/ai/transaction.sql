-- ============================================================
-- AI 账务流水表
-- 参考 llmgateway transaction / 钱包、额度、订阅的账务流水
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai."transaction" (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    order_id            BIGINT          NOT NULL DEFAULT 0,
    payment_method_id   BIGINT          NOT NULL DEFAULT 0,
    account_type        VARCHAR(32)     NOT NULL DEFAULT 'wallet',
    direction           VARCHAR(16)     NOT NULL DEFAULT 'credit',
    trade_type          VARCHAR(32)     NOT NULL DEFAULT 'topup',
    amount              DECIMAL(20,8)   NOT NULL DEFAULT 0,
    currency            VARCHAR(8)      NOT NULL DEFAULT 'USD',
    quota_delta         BIGINT          NOT NULL DEFAULT 0,
    balance_before      DECIMAL(20,8)   NOT NULL DEFAULT 0,
    balance_after       DECIMAL(20,8)   NOT NULL DEFAULT 0,
    reference_no        VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_transaction_org_user_time ON ai."transaction" (organization_id, user_id, create_time);
CREATE INDEX idx_ai_transaction_order_id ON ai."transaction" (order_id);
CREATE INDEX idx_ai_transaction_trade_type ON ai."transaction" (trade_type);
CREATE INDEX idx_ai_transaction_reference_no ON ai."transaction" (reference_no);

COMMENT ON TABLE ai."transaction" IS 'AI 账务流水表';
COMMENT ON COLUMN ai."transaction".id IS '流水ID';
COMMENT ON COLUMN ai."transaction".organization_id IS '组织ID';
COMMENT ON COLUMN ai."transaction".user_id IS '用户ID';
COMMENT ON COLUMN ai."transaction".project_id IS '项目ID';
COMMENT ON COLUMN ai."transaction".order_id IS '关联订单ID';
COMMENT ON COLUMN ai."transaction".payment_method_id IS '关联支付方式ID';
COMMENT ON COLUMN ai."transaction".account_type IS '账本类型：wallet/quota/subscription/referral';
COMMENT ON COLUMN ai."transaction".direction IS '方向：credit/debit';
COMMENT ON COLUMN ai."transaction".trade_type IS '交易类型：topup/payment/refund/consume/reward/adjust';
COMMENT ON COLUMN ai."transaction".amount IS '金额变动';
COMMENT ON COLUMN ai."transaction".currency IS '货币';
COMMENT ON COLUMN ai."transaction".quota_delta IS '额度变动';
COMMENT ON COLUMN ai."transaction".balance_before IS '变动前余额';
COMMENT ON COLUMN ai."transaction".balance_after IS '变动后余额';
COMMENT ON COLUMN ai."transaction".reference_no IS '参考号';
COMMENT ON COLUMN ai."transaction".status IS '状态：1=成功 2=处理中 3=失败';
COMMENT ON COLUMN ai."transaction".metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai."transaction".create_time IS '创建时间';
