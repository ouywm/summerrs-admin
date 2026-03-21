-- ============================================================
-- AI 插件表
-- 参考 APIPark plugin_partition / bifrost config_plugins
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.plugin (
    id                  BIGSERIAL       PRIMARY KEY,
    plugin_code         VARCHAR(64)     NOT NULL,
    plugin_name         VARCHAR(128)    NOT NULL DEFAULT '',
    plugin_type         VARCHAR(32)     NOT NULL DEFAULT 'middleware',
    runtime_type        VARCHAR(32)     NOT NULL DEFAULT 'wasm',
    version             VARCHAR(32)     NOT NULL DEFAULT '',
    entrypoint          VARCHAR(255)    NOT NULL DEFAULT '',
    config_schema       JSONB           NOT NULL DEFAULT '{}'::jsonb,
    default_config      JSONB           NOT NULL DEFAULT '{}'::jsonb,
    status              SMALLINT        NOT NULL DEFAULT 1,
    signed              BOOLEAN         NOT NULL DEFAULT FALSE,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_plugin_code ON ai.plugin (plugin_code);
CREATE INDEX idx_ai_plugin_type_status ON ai.plugin (plugin_type, status);
CREATE INDEX idx_ai_plugin_runtime_type ON ai.plugin (runtime_type);

COMMENT ON TABLE ai.plugin IS 'AI 插件表';
COMMENT ON COLUMN ai.plugin.id IS '插件ID';
COMMENT ON COLUMN ai.plugin.plugin_code IS '插件编码';
COMMENT ON COLUMN ai.plugin.plugin_name IS '插件名称';
COMMENT ON COLUMN ai.plugin.plugin_type IS '插件类型：middleware/router/auth/guardrail/logger/tool';
COMMENT ON COLUMN ai.plugin.runtime_type IS '运行时：wasm/lua/http/native';
COMMENT ON COLUMN ai.plugin.version IS '插件版本';
COMMENT ON COLUMN ai.plugin.entrypoint IS '插件入口';
COMMENT ON COLUMN ai.plugin.config_schema IS '配置契约（JSON Schema）';
COMMENT ON COLUMN ai.plugin.default_config IS '默认配置（JSON）';
COMMENT ON COLUMN ai.plugin.status IS '状态：1=启用 2=禁用 3=下线';
COMMENT ON COLUMN ai.plugin.signed IS '是否签名校验通过';
COMMENT ON COLUMN ai.plugin.remark IS '备注';
COMMENT ON COLUMN ai.plugin.create_by IS '创建人';
COMMENT ON COLUMN ai.plugin.create_time IS '创建时间';
COMMENT ON COLUMN ai.plugin.update_by IS '更新人';
COMMENT ON COLUMN ai.plugin.update_time IS '更新时间';
