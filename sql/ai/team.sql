-- ============================================================
-- AI 团队表
-- 参考 hadrian teams / LiteLLM TeamTable
-- 团队属于组织，可挂多个项目
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.team (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL REFERENCES ai.organization(id) ON DELETE CASCADE,
    owner_user_id       BIGINT          NOT NULL DEFAULT 0,
    team_code           VARCHAR(64)     NOT NULL,
    team_name           VARCHAR(128)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    sort_order          INT             NOT NULL DEFAULT 0,
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

CREATE UNIQUE INDEX uk_ai_team_org_team_code ON ai.team (organization_id, team_code);
CREATE INDEX idx_ai_team_org_status_sort_order ON ai.team (organization_id, status, sort_order);
CREATE INDEX idx_ai_team_owner_user_id ON ai.team (owner_user_id);

COMMENT ON TABLE ai.team IS 'AI 团队表（组织下的协作团队）';
COMMENT ON COLUMN ai.team.id IS '团队ID';
COMMENT ON COLUMN ai.team.organization_id IS '所属组织ID';
COMMENT ON COLUMN ai.team.owner_user_id IS '团队负责人用户ID';
COMMENT ON COLUMN ai.team.team_code IS '团队编码（组织内唯一）';
COMMENT ON COLUMN ai.team.team_name IS '团队名称';
COMMENT ON COLUMN ai.team.status IS '状态：1=启用 2=禁用 3=归档';
COMMENT ON COLUMN ai.team.sort_order IS '排序';
COMMENT ON COLUMN ai.team.quota_limit IS '团队总额度上限（0=不限制）';
COMMENT ON COLUMN ai.team.used_quota IS '团队累计已用额度';
COMMENT ON COLUMN ai.team.daily_quota_limit IS '团队日额度上限';
COMMENT ON COLUMN ai.team.monthly_quota_limit IS '团队月额度上限';
COMMENT ON COLUMN ai.team.request_count IS '团队累计请求数';
COMMENT ON COLUMN ai.team.settings IS '团队级设置（JSON）';
COMMENT ON COLUMN ai.team.remark IS '备注';
COMMENT ON COLUMN ai.team.create_by IS '创建人';
COMMENT ON COLUMN ai.team.create_time IS '创建时间';
COMMENT ON COLUMN ai.team.update_by IS '更新人';
COMMENT ON COLUMN ai.team.update_time IS '更新时间';
