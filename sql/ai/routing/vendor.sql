-- ============================================================
-- AI 供应商表
-- 对标 new-api vendor / one-hub model_ownedby
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.vendor (
    id              BIGSERIAL       PRIMARY KEY,
    vendor_code     VARCHAR(64)     NOT NULL,
    vendor_name     VARCHAR(128)    NOT NULL DEFAULT '',
    api_style       VARCHAR(64)     NOT NULL DEFAULT '',
    icon            VARCHAR(512)    NOT NULL DEFAULT '',
    description     VARCHAR(500)    NOT NULL DEFAULT '',
    base_url        VARCHAR(512)    NOT NULL DEFAULT '',
    doc_url         VARCHAR(512)    NOT NULL DEFAULT '',
    metadata        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    vendor_sort     INT             NOT NULL DEFAULT 0,
    enabled         BOOLEAN         NOT NULL DEFAULT TRUE,
    remark          VARCHAR(500)    NOT NULL DEFAULT '',
    create_by       VARCHAR(64)     NOT NULL DEFAULT '',
    create_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by       VARCHAR(64)     NOT NULL DEFAULT '',
    update_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_vendor_vendor_code ON ai.vendor (vendor_code);

COMMENT ON TABLE ai.vendor IS 'AI 供应商表（模型供应商元数据，用于展示与分类）';
COMMENT ON COLUMN ai.vendor.id IS '供应商ID';
COMMENT ON COLUMN ai.vendor.vendor_code IS '供应商编码（唯一）';
COMMENT ON COLUMN ai.vendor.vendor_name IS '供应商名称';
COMMENT ON COLUMN ai.vendor.api_style IS 'API 风格（如 openai-compatible / anthropic-native / gemini-native）';
COMMENT ON COLUMN ai.vendor.icon IS '图标 URL 或 SVG';
COMMENT ON COLUMN ai.vendor.description IS '供应商简介';
COMMENT ON COLUMN ai.vendor.base_url IS '官方默认 API 地址';
COMMENT ON COLUMN ai.vendor.doc_url IS '官方文档地址';
COMMENT ON COLUMN ai.vendor.metadata IS '供应商扩展元数据（JSON）';
COMMENT ON COLUMN ai.vendor.vendor_sort IS '排序（越小越靠前）';
COMMENT ON COLUMN ai.vendor.enabled IS '是否启用';
COMMENT ON COLUMN ai.vendor.remark IS '备注';
COMMENT ON COLUMN ai.vendor.create_by IS '创建人';
COMMENT ON COLUMN ai.vendor.create_time IS '创建时间';
COMMENT ON COLUMN ai.vendor.update_by IS '更新人';
COMMENT ON COLUMN ai.vendor.update_time IS '更新时间';

INSERT INTO ai.vendor (vendor_code, vendor_name, api_style, base_url, doc_url, metadata, vendor_sort, enabled, create_by) VALUES
    ('openai',      'OpenAI',       'openai-compatible', 'https://api.openai.com',                       'https://platform.openai.com/docs',                     '{}'::jsonb, 1,  TRUE, 'system'),
    ('anthropic',   'Anthropic',    'anthropic-native',  'https://api.anthropic.com',                    'https://docs.anthropic.com',                           '{}'::jsonb, 2,  TRUE, 'system'),
    ('azure',       'Azure OpenAI', 'openai-compatible', '',                                              'https://learn.microsoft.com/azure/ai-services/openai', '{}'::jsonb, 3,  TRUE, 'system'),
    ('baidu',       'Baidu (文心)', 'openai-compatible', 'https://aip.baidubce.com',                     'https://cloud.baidu.com/doc/WENXINWORKSHOP',           '{}'::jsonb, 4,  TRUE, 'system'),
    ('ali',         'Ali (通义)',   'openai-compatible', 'https://dashscope.aliyuncs.com',               'https://help.aliyun.com/zh/model-studio',              '{}'::jsonb, 5,  TRUE, 'system'),
    ('google',      'Google',       'gemini-native',     'https://generativelanguage.googleapis.com',    'https://ai.google.dev/docs',                           '{}'::jsonb, 6,  TRUE, 'system'),
    ('ollama',      'Ollama',       'openai-compatible', 'http://localhost:11434',                       'https://ollama.com',                                   '{}'::jsonb, 7,  TRUE, 'system'),
    ('deepseek',    'DeepSeek',     'openai-compatible', 'https://api.deepseek.com',                     'https://platform.deepseek.com/docs',                   '{}'::jsonb, 8,  TRUE, 'system'),
    ('groq',        'Groq',         'openai-compatible', 'https://api.groq.com/openai',                  'https://console.groq.com/docs',                        '{}'::jsonb, 9,  TRUE, 'system'),
    ('openrouter',  'OpenRouter',   'openai-compatible', 'https://openrouter.ai/api',                    'https://openrouter.ai/docs',                           '{}'::jsonb, 10, TRUE, 'system')
ON CONFLICT (vendor_code) DO NOTHING;
