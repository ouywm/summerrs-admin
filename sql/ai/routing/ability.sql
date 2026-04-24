-- ============================================================
-- AI 渠道能力表（路由索引）
-- 由 ai.channel.models 按 endpoint_scope 展开生成
-- (channel_group, endpoint_scope, model) -> 可选渠道集合
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.ability (
    id              BIGSERIAL       PRIMARY KEY,
    channel_group   VARCHAR(64)     NOT NULL,
    endpoint_scope  VARCHAR(32)     NOT NULL DEFAULT 'chat',
    model           VARCHAR(128)    NOT NULL,
    channel_id      BIGINT          NOT NULL,
    enabled         BOOLEAN         NOT NULL DEFAULT TRUE,
    priority        INT             NOT NULL DEFAULT 0,
    weight          INT             NOT NULL DEFAULT 1 CHECK (weight >= 1),
    route_config    JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_ability_group_scope_model_channel
    ON ai.ability (channel_group, endpoint_scope, model, channel_id);
CREATE INDEX idx_ai_ability_channel_id ON ai.ability (channel_id);
CREATE INDEX idx_ai_ability_priority ON ai.ability (priority);
CREATE INDEX idx_ai_ability_route ON ai.ability (channel_group, endpoint_scope, model, enabled, priority);

COMMENT ON TABLE ai.ability IS 'AI 渠道能力表（路由索引，由渠道模型能力展开生成）';
COMMENT ON COLUMN ai.ability.id IS '能力ID';
COMMENT ON COLUMN ai.ability.channel_group IS '渠道分组（对应 ai.channel.channel_group）';
COMMENT ON COLUMN ai.ability.endpoint_scope IS 'endpoint 范围：chat/responses/embeddings/images/audio/batches 等';
COMMENT ON COLUMN ai.ability.model IS '模型标识（请求侧模型名）';
COMMENT ON COLUMN ai.ability.channel_id IS '渠道ID（ai.channel.id）';
COMMENT ON COLUMN ai.ability.enabled IS '是否启用';
COMMENT ON COLUMN ai.ability.priority IS '路由优先级（覆盖渠道默认值）';
COMMENT ON COLUMN ai.ability.weight IS '路由权重（覆盖渠道默认值）';
COMMENT ON COLUMN ai.ability.route_config IS '能力级路由扩展配置（JSON）';
COMMENT ON COLUMN ai.ability.create_time IS '创建时间';
COMMENT ON COLUMN ai.ability.update_time IS '更新时间';
