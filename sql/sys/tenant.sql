-- ============================================================
-- 租户主表
-- 平台级租户控制面主体，独立于 ai.organization
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.tenant (
    id                      BIGSERIAL       PRIMARY KEY,
    tenant_id               VARCHAR(64)     NOT NULL,
    tenant_name             VARCHAR(128)    NOT NULL,
    default_isolation_level SMALLINT        NOT NULL DEFAULT 1,
    contact_name            VARCHAR(64)     NOT NULL DEFAULT '',
    contact_email           VARCHAR(128)    NOT NULL DEFAULT '',
    contact_phone           VARCHAR(32)     NOT NULL DEFAULT '',
    expire_time             TIMESTAMP,
    status                  SMALLINT        NOT NULL DEFAULT 1,
    config                  JSONB           NOT NULL DEFAULT '{}'::jsonb,
    metadata                JSONB           NOT NULL DEFAULT '{}'::jsonb,
    remark                  VARCHAR(500)    NOT NULL DEFAULT '',
    create_by               VARCHAR(64)     NOT NULL DEFAULT '',
    create_time             TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by               VARCHAR(64)     NOT NULL DEFAULT '',
    update_time             TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_tenant_tenant_id ON sys.tenant (tenant_id);
CREATE INDEX idx_sys_tenant_status ON sys.tenant (status);
CREATE INDEX idx_sys_tenant_expire_time ON sys.tenant (expire_time);

COMMENT ON TABLE sys.tenant IS '租户主表';
COMMENT ON COLUMN sys.tenant.id IS '主键ID';
COMMENT ON COLUMN sys.tenant.tenant_id IS '租户业务唯一标识，供 Header/JWT/Sharding 运行时使用';
COMMENT ON COLUMN sys.tenant.tenant_name IS '租户名称';
COMMENT ON COLUMN sys.tenant.default_isolation_level IS '默认隔离级别：1-共享行 2-独立表 3-独立Schema 4-独立库';
COMMENT ON COLUMN sys.tenant.contact_name IS '联系人姓名';
COMMENT ON COLUMN sys.tenant.contact_email IS '联系人邮箱';
COMMENT ON COLUMN sys.tenant.contact_phone IS '联系人手机号';
COMMENT ON COLUMN sys.tenant.expire_time IS '租户到期时间';
COMMENT ON COLUMN sys.tenant.status IS '状态：1-正常 2-禁用 3-待开通 4-已归档';
COMMENT ON COLUMN sys.tenant.config IS '租户业务配置（JSON）';
COMMENT ON COLUMN sys.tenant.metadata IS '租户扩展元数据（JSON）';
COMMENT ON COLUMN sys.tenant.remark IS '备注';
COMMENT ON COLUMN sys.tenant.create_by IS '创建人';
COMMENT ON COLUMN sys.tenant.create_time IS '创建时间';
COMMENT ON COLUMN sys.tenant.update_by IS '更新人';
COMMENT ON COLUMN sys.tenant.update_time IS '更新时间';
