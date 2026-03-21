-- ============================================================
-- AI 插件绑定表
-- 控制插件挂载到组织、项目、路由规则或执行阶段
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.plugin_binding (
    id                  BIGSERIAL       PRIMARY KEY,
    plugin_id           BIGINT          NOT NULL REFERENCES ai.plugin(id) ON DELETE CASCADE,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    routing_rule_id     BIGINT          NOT NULL DEFAULT 0,
    binding_point       VARCHAR(32)     NOT NULL DEFAULT 'request',
    exec_order          INT             NOT NULL DEFAULT 0,
    enabled             BOOLEAN         NOT NULL DEFAULT TRUE,
    config              JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_plugin_binding_scope ON ai.plugin_binding (plugin_id, organization_id, project_id, routing_rule_id, binding_point);
CREATE INDEX idx_ai_plugin_binding_plugin_id ON ai.plugin_binding (plugin_id);
CREATE INDEX idx_ai_plugin_binding_scope_points ON ai.plugin_binding (organization_id, project_id, binding_point);
CREATE INDEX idx_ai_plugin_binding_enabled_order ON ai.plugin_binding (enabled, exec_order);

COMMENT ON TABLE ai.plugin_binding IS 'AI 插件绑定表';
COMMENT ON COLUMN ai.plugin_binding.id IS '绑定ID';
COMMENT ON COLUMN ai.plugin_binding.plugin_id IS '插件ID';
COMMENT ON COLUMN ai.plugin_binding.organization_id IS '组织ID';
COMMENT ON COLUMN ai.plugin_binding.project_id IS '项目ID';
COMMENT ON COLUMN ai.plugin_binding.routing_rule_id IS '路由规则ID';
COMMENT ON COLUMN ai.plugin_binding.binding_point IS '绑定点：request/response/router/guardrail/audit/scheduler';
COMMENT ON COLUMN ai.plugin_binding.exec_order IS '执行顺序';
COMMENT ON COLUMN ai.plugin_binding.enabled IS '是否启用';
COMMENT ON COLUMN ai.plugin_binding.config IS '实例化配置（JSON）';
COMMENT ON COLUMN ai.plugin_binding.remark IS '备注';
COMMENT ON COLUMN ai.plugin_binding.create_by IS '创建人';
COMMENT ON COLUMN ai.plugin_binding.create_time IS '创建时间';
COMMENT ON COLUMN ai.plugin_binding.update_by IS '更新人';
COMMENT ON COLUMN ai.plugin_binding.update_time IS '更新时间';
