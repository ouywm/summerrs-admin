-- ============================================================
-- AI 配置项表
-- 参考 bifrost config_* / 平台级可配置项统一存储
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.config_entry (
    id                  BIGSERIAL       PRIMARY KEY,
    scope_type          VARCHAR(32)     NOT NULL DEFAULT 'system',
    scope_id            BIGINT          NOT NULL DEFAULT 0,
    category            VARCHAR(32)     NOT NULL DEFAULT 'system',
    config_key          VARCHAR(128)    NOT NULL,
    config_value        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    secret_ref          VARCHAR(256)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    version_no          INT             NOT NULL DEFAULT 1,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_config_entry_scope_key ON ai.config_entry (scope_type, scope_id, category, config_key);
CREATE INDEX idx_ai_config_entry_category_status ON ai.config_entry (category, status);
CREATE INDEX idx_ai_config_entry_scope ON ai.config_entry (scope_type, scope_id);

COMMENT ON TABLE ai.config_entry IS 'AI 配置项表（统一承载 provider/model/plugin/system 等配置）';
COMMENT ON COLUMN ai.config_entry.id IS '配置项ID';
COMMENT ON COLUMN ai.config_entry.scope_type IS '作用域：system/organization/project/provider/model/plugin';
COMMENT ON COLUMN ai.config_entry.scope_id IS '作用域ID';
COMMENT ON COLUMN ai.config_entry.category IS '配置分类';
COMMENT ON COLUMN ai.config_entry.config_key IS '配置键';
COMMENT ON COLUMN ai.config_entry.config_value IS '配置值（JSON）';
COMMENT ON COLUMN ai.config_entry.secret_ref IS '敏感值外部引用';
COMMENT ON COLUMN ai.config_entry.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.config_entry.version_no IS '版本号';
COMMENT ON COLUMN ai.config_entry.remark IS '备注';
COMMENT ON COLUMN ai.config_entry.create_by IS '创建人';
COMMENT ON COLUMN ai.config_entry.create_time IS '创建时间';
COMMENT ON COLUMN ai.config_entry.update_by IS '更新人';
COMMENT ON COLUMN ai.config_entry.update_time IS '更新时间';
