-- ============================================================
-- AI 组织成员表
-- 参考 hadrian org_memberships / LiteLLM_OrganizationMembership
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.org_membership (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL,
    user_id             BIGINT          NOT NULL,
    role_code           VARCHAR(32)     NOT NULL DEFAULT 'member',
    status              SMALLINT        NOT NULL DEFAULT 1,
    source              VARCHAR(32)     NOT NULL DEFAULT 'manual',
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_org_membership_org_user ON ai.org_membership (organization_id, user_id);
CREATE INDEX idx_ai_org_membership_user_id ON ai.org_membership (user_id);
CREATE INDEX idx_ai_org_membership_role_code ON ai.org_membership (role_code);

COMMENT ON TABLE ai.org_membership IS 'AI 组织成员表';
COMMENT ON COLUMN ai.org_membership.id IS '成员关系ID';
COMMENT ON COLUMN ai.org_membership.organization_id IS '组织ID';
COMMENT ON COLUMN ai.org_membership.user_id IS '用户ID';
COMMENT ON COLUMN ai.org_membership.role_code IS '组织角色：owner/admin/member/billing_viewer 等';
COMMENT ON COLUMN ai.org_membership.status IS '状态：1=正常 2=禁用 3=移除';
COMMENT ON COLUMN ai.org_membership.source IS '来源：manual/sso/scim/invite';
COMMENT ON COLUMN ai.org_membership.create_by IS '创建人';
COMMENT ON COLUMN ai.org_membership.create_time IS '创建时间';
COMMENT ON COLUMN ai.org_membership.update_by IS '更新人';
COMMENT ON COLUMN ai.org_membership.update_time IS '更新时间';
