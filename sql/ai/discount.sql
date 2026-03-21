-- ============================================================
-- AI 折扣规则表
-- 参考 llmgateway discount / 促销和阶梯折扣策略
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.discount (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    scope_type          VARCHAR(32)     NOT NULL DEFAULT 'global',
    scope_key           VARCHAR(128)    NOT NULL DEFAULT '',
    name                VARCHAR(128)    NOT NULL DEFAULT '',
    discount_type       VARCHAR(16)     NOT NULL DEFAULT 'ratio',
    discount_value      DECIMAL(20,8)   NOT NULL DEFAULT 0,
    currency            VARCHAR(8)      NOT NULL DEFAULT 'USD',
    condition_json      JSONB           NOT NULL DEFAULT '{}'::jsonb,
    priority            INT             NOT NULL DEFAULT 100,
    status              SMALLINT        NOT NULL DEFAULT 1,
    start_time          TIMESTAMPTZ,
    end_time            TIMESTAMPTZ,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_discount_scope ON ai.discount (scope_type, scope_key);
CREATE INDEX idx_ai_discount_org_project ON ai.discount (organization_id, project_id);
CREATE INDEX idx_ai_discount_status_time ON ai.discount (status, start_time, end_time);
CREATE INDEX idx_ai_discount_priority ON ai.discount (priority);

COMMENT ON TABLE ai.discount IS 'AI 折扣规则表';
COMMENT ON COLUMN ai.discount.id IS '折扣ID';
COMMENT ON COLUMN ai.discount.organization_id IS '组织ID';
COMMENT ON COLUMN ai.discount.project_id IS '项目ID';
COMMENT ON COLUMN ai.discount.scope_type IS '作用域：global/organization/project/model/provider/user';
COMMENT ON COLUMN ai.discount.scope_key IS '作用域键，如模型名/提供方编码';
COMMENT ON COLUMN ai.discount.name IS '折扣名称';
COMMENT ON COLUMN ai.discount.discount_type IS '折扣类型：ratio/fixed';
COMMENT ON COLUMN ai.discount.discount_value IS '折扣值';
COMMENT ON COLUMN ai.discount.currency IS '货币';
COMMENT ON COLUMN ai.discount.condition_json IS '生效条件（JSON）';
COMMENT ON COLUMN ai.discount.priority IS '优先级';
COMMENT ON COLUMN ai.discount.status IS '状态：1=启用 2=禁用 3=过期';
COMMENT ON COLUMN ai.discount.start_time IS '开始时间';
COMMENT ON COLUMN ai.discount.end_time IS '结束时间';
COMMENT ON COLUMN ai.discount.remark IS '备注';
COMMENT ON COLUMN ai.discount.create_by IS '创建人';
COMMENT ON COLUMN ai.discount.create_time IS '创建时间';
COMMENT ON COLUMN ai.discount.update_by IS '更新人';
COMMENT ON COLUMN ai.discount.update_time IS '更新时间';
