-- ============================================================
-- 系统角色表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys."role" (
    id          BIGSERIAL       PRIMARY KEY,
    role_name   VARCHAR(64)     NOT NULL,
    role_code   VARCHAR(64)     NOT NULL,
    description VARCHAR(256)    NOT NULL DEFAULT '',
    enabled     BOOLEAN         NOT NULL DEFAULT TRUE,
    create_time TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_role_role_code ON sys."role" (role_code);

COMMENT ON TABLE sys."role" IS '系统角色表';
COMMENT ON COLUMN sys."role".id IS '角色ID';
COMMENT ON COLUMN sys."role".role_name IS '角色名称';
COMMENT ON COLUMN sys."role".role_code IS '角色编码（唯一，如 R_ADMIN）';
COMMENT ON COLUMN sys."role".description IS '角色描述';
COMMENT ON COLUMN sys."role".enabled IS '是否启用';
COMMENT ON COLUMN sys."role".create_time IS '创建时间';
COMMENT ON COLUMN sys."role".update_time IS '更新时间';

-- ============================================================
-- 测试数据
-- ============================================================

INSERT INTO sys."role" (role_name, role_code, description)
VALUES
    ('超级管理员', 'R_SUPER', '拥有系统所有权限'),
    ('管理员',     'R_ADMIN', '拥有大部分管理权限'),
    ('普通用户',   'R_USER',  '仅拥有基本操作权限');