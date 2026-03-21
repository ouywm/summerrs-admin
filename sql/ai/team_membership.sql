-- ============================================================
-- AI 团队成员表
-- 参考 hadrian team_memberships
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.team_membership (
    id                  BIGSERIAL       PRIMARY KEY,
    team_id             BIGINT          NOT NULL REFERENCES ai.team(id) ON DELETE CASCADE,
    user_id             BIGINT          NOT NULL,
    role_code           VARCHAR(32)     NOT NULL DEFAULT 'member',
    status              SMALLINT        NOT NULL DEFAULT 1,
    source              VARCHAR(32)     NOT NULL DEFAULT 'manual',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_team_membership_team_user ON ai.team_membership (team_id, user_id);
CREATE INDEX idx_ai_team_membership_user_id ON ai.team_membership (user_id);
CREATE INDEX idx_ai_team_membership_role_code ON ai.team_membership (role_code);

COMMENT ON TABLE ai.team_membership IS 'AI 团队成员表';
COMMENT ON COLUMN ai.team_membership.id IS '成员关系ID';
COMMENT ON COLUMN ai.team_membership.team_id IS '团队ID';
COMMENT ON COLUMN ai.team_membership.user_id IS '用户ID';
COMMENT ON COLUMN ai.team_membership.role_code IS '团队角色';
COMMENT ON COLUMN ai.team_membership.status IS '状态：1=正常 2=禁用 3=移除';
COMMENT ON COLUMN ai.team_membership.source IS '来源：manual/sso/scim/invite';
COMMENT ON COLUMN ai.team_membership.create_by IS '创建人';
COMMENT ON COLUMN ai.team_membership.create_time IS '创建时间';
COMMENT ON COLUMN ai.team_membership.update_by IS '更新人';
COMMENT ON COLUMN ai.team_membership.update_time IS '更新时间';
