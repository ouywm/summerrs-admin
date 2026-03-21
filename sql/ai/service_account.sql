-- ============================================================
-- AI 服务账号表
-- 参考 hadrian service_accounts / SaaS gateway 机器身份设计
-- 用于项目机器人、CI/CD、批处理、内部服务等非人类调用主体
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.service_account (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL REFERENCES ai.organization(id) ON DELETE CASCADE,
    team_id             BIGINT          REFERENCES ai.team(id) ON DELETE SET NULL,
    project_id          BIGINT          REFERENCES ai.project(id) ON DELETE SET NULL,
    service_code        VARCHAR(64)     NOT NULL,
    service_name        VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    description         VARCHAR(500)    NOT NULL DEFAULT '',
    role_codes          JSONB           NOT NULL DEFAULT '[]'::jsonb,
    allowed_models      JSONB           NOT NULL DEFAULT '[]'::jsonb,
    quota_limit         BIGINT          NOT NULL DEFAULT 0,
    used_quota          BIGINT          NOT NULL DEFAULT 0,
    daily_quota_limit   BIGINT          NOT NULL DEFAULT 0,
    monthly_quota_limit BIGINT          NOT NULL DEFAULT 0,
    request_count       BIGINT          NOT NULL DEFAULT 0,
    access_time         TIMESTAMPTZ,
    expires_at          TIMESTAMPTZ,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_service_account_org_code ON ai.service_account (organization_id, service_code);
CREATE INDEX idx_ai_service_account_team_id ON ai.service_account (team_id);
CREATE INDEX idx_ai_service_account_project_id ON ai.service_account (project_id);
CREATE INDEX idx_ai_service_account_status ON ai.service_account (status);

COMMENT ON TABLE ai.service_account IS 'AI 服务账号表（机器身份/机器人账号）';
COMMENT ON COLUMN ai.service_account.id IS '服务账号ID';
COMMENT ON COLUMN ai.service_account.organization_id IS '所属组织ID';
COMMENT ON COLUMN ai.service_account.team_id IS '所属团队ID（可为空）';
COMMENT ON COLUMN ai.service_account.project_id IS '所属项目ID（可为空）';
COMMENT ON COLUMN ai.service_account.service_code IS '服务账号编码（组织内唯一）';
COMMENT ON COLUMN ai.service_account.service_name IS '服务账号名称';
COMMENT ON COLUMN ai.service_account.status IS '状态：1=启用 2=禁用 3=过期';
COMMENT ON COLUMN ai.service_account.description IS '描述';
COMMENT ON COLUMN ai.service_account.role_codes IS '角色列表（JSON 数组）';
COMMENT ON COLUMN ai.service_account.allowed_models IS '允许模型白名单（JSON 数组，空数组=不限制）';
COMMENT ON COLUMN ai.service_account.quota_limit IS '服务账号总额度上限（0=不限制）';
COMMENT ON COLUMN ai.service_account.used_quota IS '服务账号累计已用额度';
COMMENT ON COLUMN ai.service_account.daily_quota_limit IS '服务账号日额度上限';
COMMENT ON COLUMN ai.service_account.monthly_quota_limit IS '服务账号月额度上限';
COMMENT ON COLUMN ai.service_account.request_count IS '服务账号累计请求数';
COMMENT ON COLUMN ai.service_account.access_time IS '最近访问时间';
COMMENT ON COLUMN ai.service_account.expires_at IS '过期时间';
COMMENT ON COLUMN ai.service_account.remark IS '备注';
COMMENT ON COLUMN ai.service_account.create_by IS '创建人';
COMMENT ON COLUMN ai.service_account.create_time IS '创建时间';
COMMENT ON COLUMN ai.service_account.update_by IS '更新人';
COMMENT ON COLUMN ai.service_account.update_time IS '更新时间';
