-- ============================================================
-- AI 分组倍率与策略表
-- 起点来自 one-api/new-api 的 group ratio，但增强为轻量组策略表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.group_ratio (
    id                  BIGSERIAL       PRIMARY KEY,
    group_code          VARCHAR(64)     NOT NULL,
    group_name          VARCHAR(128)    NOT NULL DEFAULT '',
    ratio               DECIMAL(10,4)   NOT NULL DEFAULT 1.0,
    enabled             BOOLEAN         NOT NULL DEFAULT TRUE,
    model_whitelist     JSONB           NOT NULL DEFAULT '[]'::jsonb,
    model_blacklist     JSONB           NOT NULL DEFAULT '[]'::jsonb,
    endpoint_scopes     JSONB           NOT NULL DEFAULT '[]'::jsonb,
    fallback_group_code VARCHAR(64)     NOT NULL DEFAULT '',
    policy              JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_group_ratio_group_code ON ai.group_ratio (group_code);
CREATE INDEX idx_ai_group_ratio_enabled ON ai.group_ratio (enabled);
CREATE INDEX idx_ai_group_ratio_fallback_group_code ON ai.group_ratio (fallback_group_code);

COMMENT ON TABLE ai.group_ratio IS 'AI 分组倍率与策略表（同一分组的价格、模型权限和兜底策略）';
COMMENT ON COLUMN ai.group_ratio.id IS '分组ID';
COMMENT ON COLUMN ai.group_ratio.group_code IS '分组编码（唯一）';
COMMENT ON COLUMN ai.group_ratio.group_name IS '分组名称';
COMMENT ON COLUMN ai.group_ratio.ratio IS '计费倍率（1.0=标准价）';
COMMENT ON COLUMN ai.group_ratio.enabled IS '是否启用';
COMMENT ON COLUMN ai.group_ratio.model_whitelist IS '分组级允许模型列表（JSON 数组，空=不限制）';
COMMENT ON COLUMN ai.group_ratio.model_blacklist IS '分组级禁用模型列表（JSON 数组）';
COMMENT ON COLUMN ai.group_ratio.endpoint_scopes IS '分组级允许 endpoint 范围（JSON 数组，空=不限制）';
COMMENT ON COLUMN ai.group_ratio.fallback_group_code IS '请求不满足规则时的降级目标分组';
COMMENT ON COLUMN ai.group_ratio.policy IS '组策略 JSON（如固定渠道、灰度开关、客户端限制等）';
COMMENT ON COLUMN ai.group_ratio.remark IS '备注';
COMMENT ON COLUMN ai.group_ratio.create_by IS '创建人';
COMMENT ON COLUMN ai.group_ratio.create_time IS '创建时间';
COMMENT ON COLUMN ai.group_ratio.update_by IS '更新人';
COMMENT ON COLUMN ai.group_ratio.update_time IS '更新时间';

INSERT INTO ai.group_ratio (
    group_code,
    group_name,
    ratio,
    enabled,
    model_whitelist,
    model_blacklist,
    endpoint_scopes,
    fallback_group_code,
    policy,
    remark,
    create_by
) VALUES
    ('default', '默认分组', 1.0, TRUE, '[]'::jsonb, '[]'::jsonb, '[]'::jsonb, '', '{}'::jsonb, '所有用户默认分组', 'system'),
    ('vip',     'VIP 分组',  0.8, TRUE, '[]'::jsonb, '[]'::jsonb, '[]'::jsonb, 'default', '{}'::jsonb, 'VIP 用户享受优惠倍率', 'system')
ON CONFLICT (group_code) DO NOTHING;
