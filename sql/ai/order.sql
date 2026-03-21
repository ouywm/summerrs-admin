-- ============================================================
-- AI 订单表
-- 参考 one-hub Order / 商业化统一订单主表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai."order" (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    subscription_id     BIGINT          NOT NULL DEFAULT 0,
    payment_method_id   BIGINT          NOT NULL DEFAULT 0,
    order_no            VARCHAR(64)     NOT NULL,
    external_order_no   VARCHAR(128)    NOT NULL DEFAULT '',
    order_type          VARCHAR(32)     NOT NULL DEFAULT 'topup',
    subject             VARCHAR(255)    NOT NULL DEFAULT '',
    currency            VARCHAR(8)      NOT NULL DEFAULT 'USD',
    amount              DECIMAL(20,8)   NOT NULL DEFAULT 0,
    quota_amount        BIGINT          NOT NULL DEFAULT 0,
    discount_amount     DECIMAL(20,8)   NOT NULL DEFAULT 0,
    fee_amount          DECIMAL(20,8)   NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    payment_status      VARCHAR(32)     NOT NULL DEFAULT 'pending',
    source              VARCHAR(32)     NOT NULL DEFAULT 'system',
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    paid_time           TIMESTAMPTZ,
    expire_time         TIMESTAMPTZ,
    close_time          TIMESTAMPTZ,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_order_order_no ON ai."order" (order_no);
CREATE INDEX idx_ai_order_org_user_status ON ai."order" (organization_id, user_id, status);
CREATE INDEX idx_ai_order_project_id ON ai."order" (project_id);
CREATE INDEX idx_ai_order_subscription_id ON ai."order" (subscription_id);
CREATE INDEX idx_ai_order_paid_time ON ai."order" (paid_time);

COMMENT ON TABLE ai."order" IS 'AI 订单表';
COMMENT ON COLUMN ai."order".id IS '订单ID';
COMMENT ON COLUMN ai."order".organization_id IS '组织ID';
COMMENT ON COLUMN ai."order".user_id IS '用户ID';
COMMENT ON COLUMN ai."order".project_id IS '项目ID';
COMMENT ON COLUMN ai."order".subscription_id IS '关联订阅ID';
COMMENT ON COLUMN ai."order".payment_method_id IS '支付方式ID';
COMMENT ON COLUMN ai."order".order_no IS '平台订单号';
COMMENT ON COLUMN ai."order".external_order_no IS '外部交易单号';
COMMENT ON COLUMN ai."order".order_type IS '订单类型：topup/subscription/refund/manual_adjust/package';
COMMENT ON COLUMN ai."order".subject IS '订单标题';
COMMENT ON COLUMN ai."order".currency IS '货币';
COMMENT ON COLUMN ai."order".amount IS '订单金额';
COMMENT ON COLUMN ai."order".quota_amount IS '对应额度';
COMMENT ON COLUMN ai."order".discount_amount IS '优惠金额';
COMMENT ON COLUMN ai."order".fee_amount IS '手续费';
COMMENT ON COLUMN ai."order".status IS '状态：1=待支付 2=已支付 3=失败 4=关闭 5=退款';
COMMENT ON COLUMN ai."order".payment_status IS '支付状态';
COMMENT ON COLUMN ai."order".source IS '订单来源';
COMMENT ON COLUMN ai."order".metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai."order".paid_time IS '支付时间';
COMMENT ON COLUMN ai."order".expire_time IS '订单过期时间';
COMMENT ON COLUMN ai."order".close_time IS '关闭时间';
COMMENT ON COLUMN ai."order".remark IS '备注';
COMMENT ON COLUMN ai."order".create_by IS '创建人';
COMMENT ON COLUMN ai."order".create_time IS '创建时间';
COMMENT ON COLUMN ai."order".update_by IS '更新人';
COMMENT ON COLUMN ai."order".update_time IS '更新时间';
