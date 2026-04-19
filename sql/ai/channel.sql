-- ============================================================
-- AI 渠道表
-- 参考 one-api/new-api 的 channel 基线，但不再直接存储上游密钥
-- 渠道负责描述 provider 端点、路由策略与整体健康状态
-- 具体凭证和账号池放在 ai.channel_account 中
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.channel (
    id                  BIGSERIAL       PRIMARY KEY,
    name                VARCHAR(128)    NOT NULL DEFAULT '',
    channel_type        SMALLINT        NOT NULL DEFAULT 1,
    vendor_code         VARCHAR(64)     NOT NULL DEFAULT '',
    base_url            VARCHAR(512)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    models              JSONB           NOT NULL DEFAULT '[]'::jsonb,
    model_mapping       JSONB           NOT NULL DEFAULT '{}'::jsonb,
    channel_group       VARCHAR(64)     NOT NULL DEFAULT 'default',
    endpoint_scopes     JSONB           NOT NULL DEFAULT '["chat"]'::jsonb,
    capabilities        JSONB           NOT NULL DEFAULT '[]'::jsonb,
    weight              INT             NOT NULL DEFAULT 1 CHECK (weight >= 1),
    priority            INT             NOT NULL DEFAULT 0,
    config              JSONB           NOT NULL DEFAULT '{}'::jsonb,
    auto_ban            BOOLEAN         NOT NULL DEFAULT TRUE,
    test_model          VARCHAR(128)    NOT NULL DEFAULT '',
    used_quota          BIGINT          NOT NULL DEFAULT 0,
    balance             DECIMAL(20,8)   NOT NULL DEFAULT 0,
    balance_updated_at  TIMESTAMPTZ,
    response_time       INT             NOT NULL DEFAULT 0,
    success_rate        DECIMAL(8,4)    NOT NULL DEFAULT 0,
    failure_streak      INT             NOT NULL DEFAULT 0,
    last_used_at        TIMESTAMPTZ,
    last_error_at       TIMESTAMPTZ,
    last_error_code     VARCHAR(64)     NOT NULL DEFAULT '',
    last_error_message  TEXT            NOT NULL DEFAULT '',
    last_health_status  SMALLINT        NOT NULL DEFAULT 0,
    deleted_at          TIMESTAMPTZ,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_channel_channel_type ON ai.channel (channel_type);
CREATE INDEX idx_ai_channel_status ON ai.channel (status);
CREATE INDEX idx_ai_channel_channel_group ON ai.channel (channel_group);
CREATE INDEX idx_ai_channel_priority ON ai.channel (priority);
CREATE INDEX idx_ai_channel_vendor_code ON ai.channel (vendor_code);
CREATE INDEX idx_ai_channel_deleted_at ON ai.channel (deleted_at);

COMMENT ON TABLE ai.channel IS 'AI 渠道表（描述上游 provider 端点，不直接承载密钥）';
COMMENT ON COLUMN ai.channel.id IS '渠道ID';
COMMENT ON COLUMN ai.channel.name IS '渠道名称（如：OpenAI官方、DeepSeek公网代理）';
COMMENT ON COLUMN ai.channel.channel_type IS '渠道类型：1=OpenAI 3=Anthropic 14=Azure 15=Baidu 17=Ali 24=Gemini 28=Ollama';
COMMENT ON COLUMN ai.channel.vendor_code IS '供应商编码（对应 ai.vendor.vendor_code）';
COMMENT ON COLUMN ai.channel.base_url IS '上游 API 基础地址';
COMMENT ON COLUMN ai.channel.status IS '状态：1=启用 2=手动禁用 3=自动禁用 4=归档';
COMMENT ON COLUMN ai.channel.models IS '支持的模型列表（JSON 数组，如 ["gpt-4o","gpt-4o-mini"]）';
COMMENT ON COLUMN ai.channel.model_mapping IS '模型名映射（JSON，如 {"gpt-4": "gpt-4-turbo"}）';
COMMENT ON COLUMN ai.channel.channel_group IS '渠道分组（用户分组命中后按此分组做路由）';
COMMENT ON COLUMN ai.channel.endpoint_scopes IS '该渠道支持的 endpoint 范围（JSON 数组，如 ["chat","responses","embeddings"]）';
COMMENT ON COLUMN ai.channel.capabilities IS '渠道能力标签（JSON 数组，如 ["vision","tool_call","reasoning"]）';
COMMENT ON COLUMN ai.channel.weight IS '路由权重（同优先级内加权随机）';
COMMENT ON COLUMN ai.channel.priority IS '路由优先级（越大越优先）';
COMMENT ON COLUMN ai.channel.config IS '渠道扩展配置（JSON，如 organization、region、headers、safety 等）';
COMMENT ON COLUMN ai.channel.auto_ban IS '是否启用自动禁用';
COMMENT ON COLUMN ai.channel.test_model IS '测速使用的模型';
COMMENT ON COLUMN ai.channel.used_quota IS '累计已消耗配额';
COMMENT ON COLUMN ai.channel.balance IS '渠道/供应商维度余额快照';
COMMENT ON COLUMN ai.channel.balance_updated_at IS '余额最后更新时间';
COMMENT ON COLUMN ai.channel.response_time IS '最近一次测速响应时间（毫秒）';
COMMENT ON COLUMN ai.channel.success_rate IS '近期成功率（0-100）';
COMMENT ON COLUMN ai.channel.failure_streak IS '连续失败次数';
COMMENT ON COLUMN ai.channel.last_used_at IS '最近一次被实际选中的时间';
COMMENT ON COLUMN ai.channel.last_error_at IS '最近一次错误时间';
COMMENT ON COLUMN ai.channel.last_error_code IS '最近一次错误码';
COMMENT ON COLUMN ai.channel.last_error_message IS '最近一次错误摘要';
COMMENT ON COLUMN ai.channel.last_health_status IS '健康状态：0=未知 1=健康 2=警告 3=异常';
COMMENT ON COLUMN ai.channel.deleted_at IS '软删除时间';
COMMENT ON COLUMN ai.channel.remark IS '备注';
COMMENT ON COLUMN ai.channel.create_by IS '创建人';
COMMENT ON COLUMN ai.channel.create_time IS '创建时间';
COMMENT ON COLUMN ai.channel.update_by IS '更新人';
COMMENT ON COLUMN ai.channel.update_time IS '更新时间';
