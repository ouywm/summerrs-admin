-- ============================================================
-- AI 项目表
-- 参考 hadrian projects / axonhub project / llmgateway project
-- API Key、请求、统计都可以按项目归属
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.project (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL REFERENCES ai.organization(id) ON DELETE CASCADE,
    team_id             BIGINT          REFERENCES ai.team(id) ON DELETE SET NULL,
    owner_user_id       BIGINT          NOT NULL DEFAULT 0,
    project_code        VARCHAR(64)     NOT NULL,
    project_name        VARCHAR(128)    NOT NULL DEFAULT '',
    visibility          VARCHAR(16)     NOT NULL DEFAULT 'private',
    status              SMALLINT        NOT NULL DEFAULT 1,
    quota_limit         BIGINT          NOT NULL DEFAULT 0,
    used_quota          BIGINT          NOT NULL DEFAULT 0,
    daily_quota_limit   BIGINT          NOT NULL DEFAULT 0,
    monthly_quota_limit BIGINT          NOT NULL DEFAULT 0,
    request_count       BIGINT          NOT NULL DEFAULT 0,
    settings            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_project_org_project_code ON ai.project (organization_id, project_code);
CREATE INDEX idx_ai_project_team_id ON ai.project (team_id);
CREATE INDEX idx_ai_project_org_status ON ai.project (organization_id, status);
CREATE INDEX idx_ai_project_owner_user_id ON ai.project (owner_user_id);

COMMENT ON TABLE ai.project IS 'AI 项目表（组织/团队下的业务项目）';
COMMENT ON COLUMN ai.project.id IS '项目ID';
COMMENT ON COLUMN ai.project.organization_id IS '所属组织ID';
COMMENT ON COLUMN ai.project.team_id IS '所属团队ID（可为空）';
COMMENT ON COLUMN ai.project.owner_user_id IS '项目负责人用户ID';
COMMENT ON COLUMN ai.project.project_code IS '项目编码（组织内唯一）';
COMMENT ON COLUMN ai.project.project_name IS '项目名称';
COMMENT ON COLUMN ai.project.visibility IS '可见性：private/internal/public';
COMMENT ON COLUMN ai.project.status IS '状态：1=启用 2=禁用 3=归档';
COMMENT ON COLUMN ai.project.quota_limit IS '项目总额度上限（0=不限制）';
COMMENT ON COLUMN ai.project.used_quota IS '项目累计已用额度';
COMMENT ON COLUMN ai.project.daily_quota_limit IS '项目日额度上限';
COMMENT ON COLUMN ai.project.monthly_quota_limit IS '项目月额度上限';
COMMENT ON COLUMN ai.project.request_count IS '项目累计请求数';
COMMENT ON COLUMN ai.project.settings IS '项目级设置（JSON）';
COMMENT ON COLUMN ai.project.remark IS '备注';
COMMENT ON COLUMN ai.project.create_by IS '创建人';
COMMENT ON COLUMN ai.project.create_time IS '创建时间';
COMMENT ON COLUMN ai.project.update_by IS '更新人';
COMMENT ON COLUMN ai.project.update_time IS '更新时间';
