-- ============================================================
-- AI 邀请表
-- 参考 litellm InvitationLink / 常见团队协作邀请链路
-- 支持邀请成员加入组织、团队或项目
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.invitation (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL,
    team_id             BIGINT,
    project_id          BIGINT,
    inviter_user_id     BIGINT          NOT NULL DEFAULT 0,
    invitee_user_id     BIGINT          NOT NULL DEFAULT 0,
    invitee_email       VARCHAR(255)    NOT NULL DEFAULT '',
    target_type         VARCHAR(32)     NOT NULL DEFAULT 'organization',
    role_code           VARCHAR(32)     NOT NULL DEFAULT 'member',
    invite_token_hash   VARCHAR(64)     NOT NULL,
    status              SMALLINT        NOT NULL DEFAULT 1,
    source              VARCHAR(32)     NOT NULL DEFAULT 'manual',
    expires_at          TIMESTAMPTZ     NOT NULL,
    accepted_by         BIGINT          NOT NULL DEFAULT 0,
    accepted_time       TIMESTAMPTZ,
    remark              VARCHAR(500)    NOT NULL DEFAULT '',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_invitation_token_hash ON ai.invitation (invite_token_hash);
CREATE INDEX idx_ai_invitation_org_status_expire ON ai.invitation (organization_id, status, expires_at);
CREATE INDEX idx_ai_invitation_team_id ON ai.invitation (team_id);
CREATE INDEX idx_ai_invitation_project_id ON ai.invitation (project_id);
CREATE INDEX idx_ai_invitation_invitee_email ON ai.invitation (invitee_email);

COMMENT ON TABLE ai.invitation IS 'AI 邀请表（组织/团队/项目成员邀请）';
COMMENT ON COLUMN ai.invitation.id IS '邀请ID';
COMMENT ON COLUMN ai.invitation.organization_id IS '所属组织ID';
COMMENT ON COLUMN ai.invitation.team_id IS '目标团队ID（可为空）';
COMMENT ON COLUMN ai.invitation.project_id IS '目标项目ID（可为空）';
COMMENT ON COLUMN ai.invitation.inviter_user_id IS '邀请发起人用户ID';
COMMENT ON COLUMN ai.invitation.invitee_user_id IS '被邀请用户ID（已存在用户时使用）';
COMMENT ON COLUMN ai.invitation.invitee_email IS '被邀请邮箱';
COMMENT ON COLUMN ai.invitation.target_type IS '目标类型：organization/team/project';
COMMENT ON COLUMN ai.invitation.role_code IS '加入后的角色编码';
COMMENT ON COLUMN ai.invitation.invite_token_hash IS '邀请链接令牌哈希';
COMMENT ON COLUMN ai.invitation.status IS '状态：1=待接受 2=已接受 3=已过期 4=已撤销';
COMMENT ON COLUMN ai.invitation.source IS '来源：manual/sso/scim/import';
COMMENT ON COLUMN ai.invitation.expires_at IS '过期时间';
COMMENT ON COLUMN ai.invitation.accepted_by IS '接受邀请的用户ID';
COMMENT ON COLUMN ai.invitation.accepted_time IS '接受时间';
COMMENT ON COLUMN ai.invitation.remark IS '备注';
COMMENT ON COLUMN ai.invitation.create_by IS '创建人';
COMMENT ON COLUMN ai.invitation.create_time IS '创建时间';
COMMENT ON COLUMN ai.invitation.update_by IS '更新人';
COMMENT ON COLUMN ai.invitation.update_time IS '更新时间';
