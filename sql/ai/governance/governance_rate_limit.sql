-- ============================================================
-- AI 治理限流表
-- 参考 bifrost governance_rate_limits / 多维限速治理
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.governance_rate_limit (
    id                  BIGSERIAL       PRIMARY KEY,
    scope_type          VARCHAR(32)     NOT NULL DEFAULT 'token',
    scope_id            BIGINT          NOT NULL DEFAULT 0,
    dimension           VARCHAR(32)     NOT NULL DEFAULT 'rpm',
    limit_value         BIGINT          NOT NULL DEFAULT 0,
    burst_value         BIGINT          NOT NULL DEFAULT 0,
    window_seconds      INT             NOT NULL DEFAULT 60,
    key_pattern         VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_governance_rate_limit_scope_dim ON ai.governance_rate_limit (scope_type, scope_id, dimension, key_pattern);
CREATE INDEX idx_ai_governance_rate_limit_status ON ai.governance_rate_limit (status);
CREATE INDEX idx_ai_governance_rate_limit_scope ON ai.governance_rate_limit (scope_type, scope_id);

COMMENT ON TABLE ai.governance_rate_limit IS 'AI 治理限流表';
COMMENT ON COLUMN ai.governance_rate_limit.id IS '限流ID';
COMMENT ON COLUMN ai.governance_rate_limit.scope_type IS '作用域：organization/project/user/token/service_account/channel';
COMMENT ON COLUMN ai.governance_rate_limit.scope_id IS '作用域ID';
COMMENT ON COLUMN ai.governance_rate_limit.dimension IS '维度：rpm/tpm/concurrency/daily_cost/file_upload';
COMMENT ON COLUMN ai.governance_rate_limit.limit_value IS '限制值';
COMMENT ON COLUMN ai.governance_rate_limit.burst_value IS '突发值';
COMMENT ON COLUMN ai.governance_rate_limit.window_seconds IS '窗口秒数';
COMMENT ON COLUMN ai.governance_rate_limit.key_pattern IS '细分键模式';
COMMENT ON COLUMN ai.governance_rate_limit.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.governance_rate_limit.metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai.governance_rate_limit.remark IS '备注';
COMMENT ON COLUMN ai.governance_rate_limit.create_by IS '创建人';
COMMENT ON COLUMN ai.governance_rate_limit.create_time IS '创建时间';
COMMENT ON COLUMN ai.governance_rate_limit.update_by IS '更新人';
COMMENT ON COLUMN ai.governance_rate_limit.update_time IS '更新时间';
