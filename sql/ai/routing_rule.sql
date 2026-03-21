-- ============================================================
-- AI 路由规则表
-- 参考 bifrost routing_rules / 平台路由治理规则
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.routing_rule (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    rule_code           VARCHAR(64)     NOT NULL,
    rule_name           VARCHAR(128)    NOT NULL DEFAULT '',
    priority            INT             NOT NULL DEFAULT 100,
    match_type          VARCHAR(32)     NOT NULL DEFAULT 'json',
    match_conditions    JSONB           NOT NULL DEFAULT '{}'::jsonb,
    route_strategy      VARCHAR(32)     NOT NULL DEFAULT 'weighted',
    fallback_strategy   VARCHAR(32)     NOT NULL DEFAULT 'next',
    status              SMALLINT        NOT NULL DEFAULT 1,
    start_time          TIMESTAMPTZ,
    end_time            TIMESTAMPTZ,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_routing_rule_scope_code ON ai.routing_rule (organization_id, project_id, rule_code);
CREATE INDEX idx_ai_routing_rule_status_priority ON ai.routing_rule (status, priority);
CREATE INDEX idx_ai_routing_rule_org_project ON ai.routing_rule (organization_id, project_id);

COMMENT ON TABLE ai.routing_rule IS 'AI 路由规则表';
COMMENT ON COLUMN ai.routing_rule.id IS '规则ID';
COMMENT ON COLUMN ai.routing_rule.organization_id IS '组织ID';
COMMENT ON COLUMN ai.routing_rule.project_id IS '项目ID';
COMMENT ON COLUMN ai.routing_rule.rule_code IS '规则编码';
COMMENT ON COLUMN ai.routing_rule.rule_name IS '规则名称';
COMMENT ON COLUMN ai.routing_rule.priority IS '优先级';
COMMENT ON COLUMN ai.routing_rule.match_type IS '匹配类型：json/header/model/endpoint/expr';
COMMENT ON COLUMN ai.routing_rule.match_conditions IS '匹配条件（JSON）';
COMMENT ON COLUMN ai.routing_rule.route_strategy IS '路由策略：weighted/priority/hash/latency';
COMMENT ON COLUMN ai.routing_rule.fallback_strategy IS '失败回退策略';
COMMENT ON COLUMN ai.routing_rule.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.routing_rule.start_time IS '开始生效时间';
COMMENT ON COLUMN ai.routing_rule.end_time IS '结束生效时间';
COMMENT ON COLUMN ai.routing_rule.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.routing_rule.remark IS '备注';
COMMENT ON COLUMN ai.routing_rule.create_by IS '创建人';
COMMENT ON COLUMN ai.routing_rule.create_time IS '创建时间';
COMMENT ON COLUMN ai.routing_rule.update_by IS '更新人';
COMMENT ON COLUMN ai.routing_rule.update_time IS '更新时间';
