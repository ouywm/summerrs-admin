-- ============================================================
-- AI 订阅套餐表
-- 参考 new-api/sub2api 的订阅思路，用于长期套餐而非一次性充值
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.subscription_plan (
    id                  BIGSERIAL       PRIMARY KEY,
    plan_code           VARCHAR(64)     NOT NULL,
    plan_name           VARCHAR(128)    NOT NULL DEFAULT '',
    description         VARCHAR(500)    NOT NULL DEFAULT '',
    currency            VARCHAR(8)      NOT NULL DEFAULT 'USD',
    price_amount        DECIMAL(12,2)   NOT NULL DEFAULT 0,
    quota_total         BIGINT          NOT NULL DEFAULT 0,
    quota_reset_period  VARCHAR(16)     NOT NULL DEFAULT 'never',
    quota_reset_days    INT             NOT NULL DEFAULT 0,
    duration_unit       VARCHAR(16)     NOT NULL DEFAULT 'month',
    duration_value      INT             NOT NULL DEFAULT 1,
    group_code          VARCHAR(64)     NOT NULL DEFAULT '',
    enabled             BOOLEAN         NOT NULL DEFAULT TRUE,
    sort_order          INT             NOT NULL DEFAULT 0,
    extra               JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_subscription_plan_plan_code ON ai.subscription_plan (plan_code);
CREATE INDEX idx_ai_subscription_plan_enabled_sort_order ON ai.subscription_plan (enabled, sort_order);
CREATE INDEX idx_ai_subscription_plan_group_code ON ai.subscription_plan (group_code);

COMMENT ON TABLE ai.subscription_plan IS 'AI 订阅套餐表（长期套餐定义）';
COMMENT ON COLUMN ai.subscription_plan.id IS '套餐ID';
COMMENT ON COLUMN ai.subscription_plan.plan_code IS '套餐编码（唯一）';
COMMENT ON COLUMN ai.subscription_plan.plan_name IS '套餐名称';
COMMENT ON COLUMN ai.subscription_plan.description IS '套餐描述';
COMMENT ON COLUMN ai.subscription_plan.currency IS '货币';
COMMENT ON COLUMN ai.subscription_plan.price_amount IS '售价';
COMMENT ON COLUMN ai.subscription_plan.quota_total IS '套餐总额度（0=无限）';
COMMENT ON COLUMN ai.subscription_plan.quota_reset_period IS '额度重置周期：never/daily/weekly/monthly/custom';
COMMENT ON COLUMN ai.subscription_plan.quota_reset_days IS '自定义重置天数（非 custom 时为0）';
COMMENT ON COLUMN ai.subscription_plan.duration_unit IS '订阅时长单位：day/month/year/custom';
COMMENT ON COLUMN ai.subscription_plan.duration_value IS '时长数值';
COMMENT ON COLUMN ai.subscription_plan.group_code IS '购买后附加/提升到的分组';
COMMENT ON COLUMN ai.subscription_plan.enabled IS '是否启用';
COMMENT ON COLUMN ai.subscription_plan.sort_order IS '排序';
COMMENT ON COLUMN ai.subscription_plan.extra IS '套餐扩展配置（JSON）';
COMMENT ON COLUMN ai.subscription_plan.remark IS '备注';
COMMENT ON COLUMN ai.subscription_plan.create_by IS '创建人';
COMMENT ON COLUMN ai.subscription_plan.create_time IS '创建时间';
COMMENT ON COLUMN ai.subscription_plan.update_by IS '更新人';
COMMENT ON COLUMN ai.subscription_plan.update_time IS '更新时间';
