-- ============================================================
-- AI 支付方式表
-- 参考 llmgateway payment_method / 商业化收款方式元数据
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.payment_method (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    provider_code       VARCHAR(64)     NOT NULL DEFAULT '',
    method_type         VARCHAR(32)     NOT NULL DEFAULT 'card',
    method_label        VARCHAR(128)    NOT NULL DEFAULT '',
    provider_customer_id VARCHAR(128)   NOT NULL DEFAULT '',
    provider_method_id  VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    is_default          BOOLEAN         NOT NULL DEFAULT FALSE,
    expire_month        INT             NOT NULL DEFAULT 0,
    expire_year         INT             NOT NULL DEFAULT 0,
    billing_info        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    last_used_at        TIMESTAMPTZ,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_payment_method_org_id ON ai.payment_method (organization_id);
CREATE INDEX idx_ai_payment_method_user_id ON ai.payment_method (user_id);
CREATE INDEX idx_ai_payment_method_status_default ON ai.payment_method (status, is_default);
CREATE INDEX idx_ai_payment_method_provider_method_id ON ai.payment_method (provider_method_id);

COMMENT ON TABLE ai.payment_method IS 'AI 支付方式表';
COMMENT ON COLUMN ai.payment_method.id IS '支付方式ID';
COMMENT ON COLUMN ai.payment_method.organization_id IS '组织ID';
COMMENT ON COLUMN ai.payment_method.user_id IS '用户ID';
COMMENT ON COLUMN ai.payment_method.provider_code IS '支付提供方编码';
COMMENT ON COLUMN ai.payment_method.method_type IS '支付方式类型：card/alipay/wechat/bank_transfer/crypto';
COMMENT ON COLUMN ai.payment_method.method_label IS '支付方式显示名称';
COMMENT ON COLUMN ai.payment_method.provider_customer_id IS '支付平台客户ID';
COMMENT ON COLUMN ai.payment_method.provider_method_id IS '支付平台方式ID';
COMMENT ON COLUMN ai.payment_method.status IS '状态：1=可用 2=停用 3=失效';
COMMENT ON COLUMN ai.payment_method.is_default IS '是否默认支付方式';
COMMENT ON COLUMN ai.payment_method.expire_month IS '过期月份';
COMMENT ON COLUMN ai.payment_method.expire_year IS '过期年份';
COMMENT ON COLUMN ai.payment_method.billing_info IS '账单信息（JSON）';
COMMENT ON COLUMN ai.payment_method.metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai.payment_method.last_used_at IS '最后使用时间';
COMMENT ON COLUMN ai.payment_method.create_by IS '创建人';
COMMENT ON COLUMN ai.payment_method.create_time IS '创建时间';
COMMENT ON COLUMN ai.payment_method.update_by IS '更新人';
COMMENT ON COLUMN ai.payment_method.update_time IS '更新时间';
