-- ============================================================
-- 租户成员表
-- 租户用户与租户主表的多对多关系表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS tenant;

CREATE TABLE tenant.tenant_membership (
    id               BIGSERIAL       PRIMARY KEY,
    tenant_id        VARCHAR(64)     NOT NULL,
    user_id          BIGINT          NOT NULL,
    role_code        VARCHAR(64)     NOT NULL DEFAULT '',
    is_default       BOOLEAN         NOT NULL DEFAULT FALSE,
    status           SMALLINT        NOT NULL DEFAULT 1,
    source           VARCHAR(32)     NOT NULL DEFAULT 'manual',
    joined_time      TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_access_time TIMESTAMP,
    remark           VARCHAR(500)    NOT NULL DEFAULT '',
    create_by        VARCHAR(64)     NOT NULL DEFAULT '',
    create_time      TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by        VARCHAR(64)     NOT NULL DEFAULT '',
    update_time      TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_tenant_tenant_membership_tenant_user
    ON tenant.tenant_membership (tenant_id, user_id);
CREATE UNIQUE INDEX uk_tenant_tenant_membership_user_default
    ON tenant.tenant_membership (user_id)
    WHERE is_default = TRUE AND status = 1;
CREATE INDEX idx_tenant_tenant_membership_user_status
    ON tenant.tenant_membership (user_id, status);
CREATE INDEX idx_tenant_tenant_membership_tenant_status
    ON tenant.tenant_membership (tenant_id, status);
CREATE INDEX idx_tenant_tenant_membership_role_code
    ON tenant.tenant_membership (role_code);

COMMENT ON TABLE tenant.tenant_membership IS '租户成员表';
COMMENT ON COLUMN tenant.tenant_membership.id IS '主键ID';
COMMENT ON COLUMN tenant.tenant_membership.tenant_id IS '租户业务唯一标识，对应 tenant.tenant.tenant_id';
COMMENT ON COLUMN tenant.tenant_membership.user_id IS '租户域用户ID（不绑定 sys."user"）';
COMMENT ON COLUMN tenant.tenant_membership.role_code IS '租户内成员角色编码，由业务侧自定义';
COMMENT ON COLUMN tenant.tenant_membership.is_default IS '是否为该用户默认进入的租户';
COMMENT ON COLUMN tenant.tenant_membership.status IS '状态：1-正常 2-禁用 3-移除';
COMMENT ON COLUMN tenant.tenant_membership.source IS '来源：manual-手动 invite-邀请 sso-单点登录 scim-SCIM同步 system-系统初始化';
COMMENT ON COLUMN tenant.tenant_membership.joined_time IS '加入时间';
COMMENT ON COLUMN tenant.tenant_membership.last_access_time IS '最近访问时间';
COMMENT ON COLUMN tenant.tenant_membership.remark IS '备注';
COMMENT ON COLUMN tenant.tenant_membership.create_by IS '创建人';
COMMENT ON COLUMN tenant.tenant_membership.create_time IS '创建时间';
COMMENT ON COLUMN tenant.tenant_membership.update_by IS '更新人';
COMMENT ON COLUMN tenant.tenant_membership.update_time IS '更新时间';
