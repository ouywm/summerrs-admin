-- ============================================================
-- AI 模型配置表（全局默认计费倍率与能力标记）
-- 对标 one-api ratio / new-api pricing / hadrian model_pricing
-- 这是平台默认口径；真实渠道采购价走 ai.channel_model_price(+version)
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.model_config (
    id                  BIGSERIAL       PRIMARY KEY,
    model_name          VARCHAR(128)    NOT NULL,
    display_name        VARCHAR(256)    NOT NULL DEFAULT '',
    model_type          SMALLINT        NOT NULL DEFAULT 1,
    vendor_code         VARCHAR(64)     NOT NULL DEFAULT '',
    supported_endpoints JSONB           NOT NULL DEFAULT '["chat"]'::jsonb,
    input_ratio         DECIMAL(10,4)   NOT NULL DEFAULT 1.0,
    output_ratio        DECIMAL(10,4)   NOT NULL DEFAULT 1.0,
    cached_input_ratio  DECIMAL(10,4)   NOT NULL DEFAULT 0.5,
    reasoning_ratio     DECIMAL(10,4)   NOT NULL DEFAULT 1.0,
    capabilities        JSONB           NOT NULL DEFAULT '[]'::jsonb,
    max_context         INT             NOT NULL DEFAULT 0,
    currency            VARCHAR(8)      NOT NULL DEFAULT 'USD',
    effective_from      TIMESTAMPTZ,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    enabled             BOOLEAN         NOT NULL DEFAULT TRUE,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_model_config_model_name ON ai.model_config (model_name);
CREATE INDEX idx_ai_model_config_model_type ON ai.model_config (model_type);
CREATE INDEX idx_ai_model_config_vendor_code ON ai.model_config (vendor_code);
CREATE INDEX idx_ai_model_config_enabled ON ai.model_config (enabled);

COMMENT ON TABLE ai.model_config IS 'AI 模型配置表（全局默认倍率与能力标签）';
COMMENT ON COLUMN ai.model_config.id IS '配置ID';
COMMENT ON COLUMN ai.model_config.model_name IS '模型标识（唯一）';
COMMENT ON COLUMN ai.model_config.display_name IS '模型显示名称';
COMMENT ON COLUMN ai.model_config.model_type IS '模型类型：1=chat 2=embedding 3=image 4=audio 5=reasoning';
COMMENT ON COLUMN ai.model_config.vendor_code IS '供应商编码（对应 ai.vendor.vendor_code）';
COMMENT ON COLUMN ai.model_config.supported_endpoints IS '支持的 endpoint 范围（JSON 数组）';
COMMENT ON COLUMN ai.model_config.input_ratio IS '输入 token 计费倍率';
COMMENT ON COLUMN ai.model_config.output_ratio IS '输出 token 计费倍率';
COMMENT ON COLUMN ai.model_config.cached_input_ratio IS '缓存命中 token 计费倍率';
COMMENT ON COLUMN ai.model_config.reasoning_ratio IS '推理 token 计费倍率';
COMMENT ON COLUMN ai.model_config.capabilities IS '模型能力标签（JSON 数组，如 ["vision","tool_call"]）';
COMMENT ON COLUMN ai.model_config.max_context IS '最大上下文长度';
COMMENT ON COLUMN ai.model_config.currency IS '默认成本货币';
COMMENT ON COLUMN ai.model_config.effective_from IS '默认倍率生效时间';
COMMENT ON COLUMN ai.model_config.metadata IS '模型补充元数据（JSON）';
COMMENT ON COLUMN ai.model_config.enabled IS '是否启用';
COMMENT ON COLUMN ai.model_config.remark IS '备注';
COMMENT ON COLUMN ai.model_config.create_by IS '创建人';
COMMENT ON COLUMN ai.model_config.create_time IS '创建时间';
COMMENT ON COLUMN ai.model_config.update_by IS '更新人';
COMMENT ON COLUMN ai.model_config.update_time IS '更新时间';

INSERT INTO ai.model_config (
    model_name,
    display_name,
    model_type,
    vendor_code,
    supported_endpoints,
    input_ratio,
    output_ratio,
    cached_input_ratio,
    reasoning_ratio,
    capabilities,
    max_context,
    currency,
    metadata,
    enabled,
    remark,
    create_by
) VALUES
    ('gpt-4o',                    'GPT-4o',             1, 'openai',    '["chat","responses"]'::jsonb, 16.67,  40.0,   8.33,   1.0,   '["vision","tool_call","streaming"]'::jsonb,             128000,  'USD', '{}'::jsonb, TRUE, '', 'system'),
    ('gpt-4o-mini',               'GPT-4o Mini',        1, 'openai',    '["chat","responses"]'::jsonb, 1.0,    4.0,    0.5,    1.0,   '["vision","tool_call","streaming"]'::jsonb,             128000,  'USD', '{}'::jsonb, TRUE, '', 'system'),
    ('o3-mini',                   'o3 Mini',            5, 'openai',    '["chat","responses"]'::jsonb, 7.33,   29.33,  3.67,   29.33, '["reasoning","tool_call","streaming"]'::jsonb,          200000,  'USD', '{}'::jsonb, TRUE, '', 'system'),
    ('claude-sonnet-4-20250514',  'Claude Sonnet 4',    1, 'anthropic', '["chat","responses"]'::jsonb, 20.0,   100.0,  10.0,   1.0,   '["vision","tool_call","reasoning","streaming"]'::jsonb, 200000,  'USD', '{}'::jsonb, TRUE, '', 'system'),
    ('claude-haiku-3-5-20241022', 'Claude 3.5 Haiku',   1, 'anthropic', '["chat","responses"]'::jsonb, 5.33,   26.67,  2.67,   1.0,   '["vision","tool_call","streaming"]'::jsonb,             200000,  'USD', '{}'::jsonb, TRUE, '', 'system'),
    ('gemini-2.0-flash',          'Gemini 2.0 Flash',   1, 'google',    '["chat","responses","images"]'::jsonb, 0.5, 2.67, 0.25, 1.0, '["vision","tool_call","streaming"]'::jsonb,            1048576, 'USD', '{}'::jsonb, TRUE, '', 'system'),
    ('deepseek-chat',             'DeepSeek V3',        1, 'deepseek',  '["chat"]'::jsonb,               1.83,   7.33,   0.92,   1.0,   '["tool_call","streaming"]'::jsonb,                        65536,   'USD', '{}'::jsonb, TRUE, '', 'system'),
    ('deepseek-reasoner',         'DeepSeek R1',        5, 'deepseek',  '["chat"]'::jsonb,               3.67,   14.67,  1.83,   3.67,  '["reasoning","streaming"]'::jsonb,                        65536,   'USD', '{}'::jsonb, TRUE, '', 'system')
ON CONFLICT (model_name) DO NOTHING;
