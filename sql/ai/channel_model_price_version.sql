-- ============================================================
-- AI 渠道模型价格版本表
-- 参考 axonhub channel_model_price_versions
-- 每次改价都保留历史快照，便于账务回放与毛利分析
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.channel_model_price_version (
    id                      BIGSERIAL       PRIMARY KEY,
    channel_model_price_id  BIGINT          NOT NULL REFERENCES ai.channel_model_price(id) ON DELETE CASCADE,
    channel_id              BIGINT          NOT NULL,
    model_name              VARCHAR(128)    NOT NULL,
    version_no              INT             NOT NULL DEFAULT 1,
    reference_id            VARCHAR(64)     NOT NULL,
    price_config            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    effective_start_at      TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    effective_end_at        TIMESTAMPTZ,
    status                  SMALLINT        NOT NULL DEFAULT 1,
    create_time             TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_channel_model_price_version_ref
    ON ai.channel_model_price_version (reference_id);
CREATE UNIQUE INDEX uk_ai_channel_model_price_version_no
    ON ai.channel_model_price_version (channel_model_price_id, version_no);
CREATE INDEX idx_ai_channel_model_price_version_channel_model
    ON ai.channel_model_price_version (channel_id, model_name, status);

COMMENT ON TABLE ai.channel_model_price_version IS 'AI 渠道模型价格版本表（保留每次价格变更的历史快照）';
COMMENT ON COLUMN ai.channel_model_price_version.id IS '价格版本ID';
COMMENT ON COLUMN ai.channel_model_price_version.channel_model_price_id IS '主价格ID';
COMMENT ON COLUMN ai.channel_model_price_version.channel_id IS '渠道ID冗余';
COMMENT ON COLUMN ai.channel_model_price_version.model_name IS '模型名冗余';
COMMENT ON COLUMN ai.channel_model_price_version.version_no IS '版本号';
COMMENT ON COLUMN ai.channel_model_price_version.reference_id IS '价格快照引用ID（用于记账）';
COMMENT ON COLUMN ai.channel_model_price_version.price_config IS '价格配置 JSON 快照';
COMMENT ON COLUMN ai.channel_model_price_version.effective_start_at IS '生效开始时间';
COMMENT ON COLUMN ai.channel_model_price_version.effective_end_at IS '生效结束时间（NULL=当前仍生效）';
COMMENT ON COLUMN ai.channel_model_price_version.status IS '状态：1=生效 2=归档';
COMMENT ON COLUMN ai.channel_model_price_version.create_time IS '创建时间';
