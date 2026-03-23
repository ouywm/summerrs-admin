-- ============================================================
-- AI 路由目标表
-- 参考 bifrost routing_targets / 路由规则命中的具体去向
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.routing_target (
    id                  BIGSERIAL       PRIMARY KEY,
    routing_rule_id     BIGINT          NOT NULL,
    target_type         VARCHAR(32)     NOT NULL DEFAULT 'channel',
    channel_id          BIGINT          NOT NULL DEFAULT 0,
    account_id          BIGINT          NOT NULL DEFAULT 0,
    plugin_id           BIGINT          NOT NULL DEFAULT 0,
    target_key          VARCHAR(128)    NOT NULL DEFAULT '',
    weight              INT             NOT NULL DEFAULT 1,
    priority            INT             NOT NULL DEFAULT 0,
    cooldown_seconds    INT             NOT NULL DEFAULT 0,
    config              JSONB           NOT NULL DEFAULT '{}'::jsonb,
    status              SMALLINT        NOT NULL DEFAULT 1,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_routing_target_rule_id ON ai.routing_target (routing_rule_id);
CREATE INDEX idx_ai_routing_target_channel_id ON ai.routing_target (channel_id);
CREATE INDEX idx_ai_routing_target_account_id ON ai.routing_target (account_id);
CREATE INDEX idx_ai_routing_target_status_priority ON ai.routing_target (status, priority);

COMMENT ON TABLE ai.routing_target IS 'AI 路由目标表';
COMMENT ON COLUMN ai.routing_target.id IS '目标ID';
COMMENT ON COLUMN ai.routing_target.routing_rule_id IS '所属路由规则ID';
COMMENT ON COLUMN ai.routing_target.target_type IS '目标类型：channel/account/channel_group/plugin/pipeline';
COMMENT ON COLUMN ai.routing_target.channel_id IS '渠道ID';
COMMENT ON COLUMN ai.routing_target.account_id IS '账号ID';
COMMENT ON COLUMN ai.routing_target.plugin_id IS '插件ID';
COMMENT ON COLUMN ai.routing_target.target_key IS '目标键';
COMMENT ON COLUMN ai.routing_target.weight IS '权重';
COMMENT ON COLUMN ai.routing_target.priority IS '优先级';
COMMENT ON COLUMN ai.routing_target.cooldown_seconds IS '冷却秒数';
COMMENT ON COLUMN ai.routing_target.config IS '附加配置（JSON）';
COMMENT ON COLUMN ai.routing_target.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.routing_target.create_time IS '创建时间';
COMMENT ON COLUMN ai.routing_target.update_time IS '更新时间';
