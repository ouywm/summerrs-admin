-- ============================================================
-- AI 治理预算表
-- 参考 bifrost governance_budgets / 多作用域预算治理
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.governance_budget (
    id                  BIGSERIAL       PRIMARY KEY,
    scope_type          VARCHAR(32)     NOT NULL DEFAULT 'project',
    scope_id            BIGINT          NOT NULL DEFAULT 0,
    budget_name         VARCHAR(128)    NOT NULL DEFAULT '',
    currency            VARCHAR(8)      NOT NULL DEFAULT 'USD',
    period_type         VARCHAR(32)     NOT NULL DEFAULT 'monthly',
    limit_amount        DECIMAL(20,8)   NOT NULL DEFAULT 0,
    warn_threshold      DECIMAL(10,4)   NOT NULL DEFAULT 0.8,
    hard_limit          BOOLEAN         NOT NULL DEFAULT FALSE,
    spent_amount        DECIMAL(20,8)   NOT NULL DEFAULT 0,
    reserved_amount     DECIMAL(20,8)   NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    last_reset_time     TIMESTAMPTZ,
    next_reset_time     TIMESTAMPTZ,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_governance_budget_scope_name ON ai.governance_budget (scope_type, scope_id, budget_name);
CREATE INDEX idx_ai_governance_budget_status ON ai.governance_budget (status);
CREATE INDEX idx_ai_governance_budget_next_reset_time ON ai.governance_budget (next_reset_time);

COMMENT ON TABLE ai.governance_budget IS 'AI 治理预算表';
COMMENT ON COLUMN ai.governance_budget.id IS '预算ID';
COMMENT ON COLUMN ai.governance_budget.scope_type IS '作用域：organization/team/project/user/token/service_account';
COMMENT ON COLUMN ai.governance_budget.scope_id IS '作用域ID';
COMMENT ON COLUMN ai.governance_budget.budget_name IS '预算名称';
COMMENT ON COLUMN ai.governance_budget.currency IS '货币';
COMMENT ON COLUMN ai.governance_budget.period_type IS '周期：daily/weekly/monthly/custom';
COMMENT ON COLUMN ai.governance_budget.limit_amount IS '预算上限';
COMMENT ON COLUMN ai.governance_budget.warn_threshold IS '预警阈值比例';
COMMENT ON COLUMN ai.governance_budget.hard_limit IS '是否硬限制';
COMMENT ON COLUMN ai.governance_budget.spent_amount IS '当前已花费';
COMMENT ON COLUMN ai.governance_budget.reserved_amount IS '预留金额';
COMMENT ON COLUMN ai.governance_budget.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.governance_budget.last_reset_time IS '上次重置时间';
COMMENT ON COLUMN ai.governance_budget.next_reset_time IS '下次重置时间';
COMMENT ON COLUMN ai.governance_budget.metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai.governance_budget.remark IS '备注';
COMMENT ON COLUMN ai.governance_budget.create_by IS '创建人';
COMMENT ON COLUMN ai.governance_budget.create_time IS '创建时间';
COMMENT ON COLUMN ai.governance_budget.update_by IS '更新人';
COMMENT ON COLUMN ai.governance_budget.update_time IS '更新时间';
