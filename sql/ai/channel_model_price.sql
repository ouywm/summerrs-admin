-- ============================================================
-- AI 渠道模型价格表
-- 参考 axonhub channel_model_price
-- 保存渠道当前生效的真实采购价/成本口径
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.channel_model_price (
    id              BIGSERIAL       PRIMARY KEY,
    channel_id      BIGINT          NOT NULL,
    model_name      VARCHAR(128)    NOT NULL,
    billing_mode    SMALLINT        NOT NULL DEFAULT 1,
    currency        VARCHAR(8)      NOT NULL DEFAULT 'USD',
    price_config    JSONB           NOT NULL DEFAULT '{}'::jsonb,
    reference_id    VARCHAR(64)     NOT NULL,
    status          SMALLINT        NOT NULL DEFAULT 1,
    remark          VARCHAR(500)    NOT NULL DEFAULT '',
    create_by       VARCHAR(64)     NOT NULL DEFAULT '',
    create_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by       VARCHAR(64)     NOT NULL DEFAULT '',
    update_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_channel_model_price_channel_model
    ON ai.channel_model_price (channel_id, model_name);
CREATE UNIQUE INDEX uk_ai_channel_model_price_reference_id
    ON ai.channel_model_price (reference_id);
CREATE INDEX idx_ai_channel_model_price_status ON ai.channel_model_price (status);

COMMENT ON TABLE ai.channel_model_price IS 'AI 渠道模型价格表（当前生效的渠道采购价）';
COMMENT ON COLUMN ai.channel_model_price.id IS '价格ID';
COMMENT ON COLUMN ai.channel_model_price.channel_id IS '渠道ID';
COMMENT ON COLUMN ai.channel_model_price.model_name IS '模型名';
COMMENT ON COLUMN ai.channel_model_price.billing_mode IS '计费模式：1=按 token 2=按请求 3=按图片/音频/视频单位';
COMMENT ON COLUMN ai.channel_model_price.currency IS '价格货币';
COMMENT ON COLUMN ai.channel_model_price.price_config IS '价格配置 JSON（如 input/output/cache/reasoning 等单价）';
COMMENT ON COLUMN ai.channel_model_price.reference_id IS '价格快照引用ID，记账时落到 ai.log.price_reference';
COMMENT ON COLUMN ai.channel_model_price.status IS '状态：1=启用 2=停用';
COMMENT ON COLUMN ai.channel_model_price.remark IS '备注';
COMMENT ON COLUMN ai.channel_model_price.create_by IS '创建人';
COMMENT ON COLUMN ai.channel_model_price.create_time IS '创建时间';
COMMENT ON COLUMN ai.channel_model_price.update_by IS '更新人';
COMMENT ON COLUMN ai.channel_model_price.update_time IS '更新时间';
