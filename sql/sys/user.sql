-- ============================================================
-- 系统用户表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys."user" (
    id          BIGSERIAL       PRIMARY KEY,
    user_name   VARCHAR(64)     NOT NULL,
    password    VARCHAR(256)    NOT NULL,
    nick_name   VARCHAR(64)     NOT NULL DEFAULT '',
    gender      SMALLINT        NOT NULL DEFAULT 0,
    phone       VARCHAR(32)     NOT NULL DEFAULT '',
    phone_verified BOOLEAN      NOT NULL DEFAULT FALSE,
    email       VARCHAR(128)    NOT NULL DEFAULT '',
    email_verified BOOLEAN      NOT NULL DEFAULT FALSE,
    avatar      VARCHAR(512)    NOT NULL DEFAULT '',
    login_fail_count INT        NOT NULL DEFAULT 0,
    locked_until TIMESTAMP,
    last_login_ip INET,
    last_login_time TIMESTAMP,
    password_update_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status      SMALLINT        NOT NULL DEFAULT 1,
    create_by   VARCHAR(64)     NOT NULL DEFAULT '',
    create_time TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by   VARCHAR(64)     NOT NULL DEFAULT '',
    update_time TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_user_user_name ON sys."user" (user_name);
CREATE UNIQUE INDEX uk_sys_user_phone ON sys."user" (phone) WHERE phone <> '';
CREATE UNIQUE INDEX uk_sys_user_email ON sys."user" (email) WHERE email <> '';

COMMENT ON TABLE sys."user" IS '系统用户表';
COMMENT ON COLUMN sys."user".id IS '用户ID';
COMMENT ON COLUMN sys."user".user_name IS '用户名（登录账号，唯一）';
COMMENT ON COLUMN sys."user".password IS '密码（加密存储）';
COMMENT ON COLUMN sys."user".nick_name IS '昵称';
COMMENT ON COLUMN sys."user".gender IS '性别：0-未知 1-男 2-女';
COMMENT ON COLUMN sys."user".phone IS '手机号';
COMMENT ON COLUMN sys."user".phone_verified IS '手机号是否已验证';
COMMENT ON COLUMN sys."user".email IS '邮箱';
COMMENT ON COLUMN sys."user".email_verified IS '邮箱是否已验证';
COMMENT ON COLUMN sys."user".avatar IS '头像URL';
COMMENT ON COLUMN sys."user".login_fail_count IS '连续登录失败次数';
COMMENT ON COLUMN sys."user".locked_until IS '账号锁定到期时间';
COMMENT ON COLUMN sys."user".last_login_ip IS '最近一次登录IP';
COMMENT ON COLUMN sys."user".last_login_time IS '最近一次登录时间';
COMMENT ON COLUMN sys."user".password_update_time IS '最近一次密码更新时间';
COMMENT ON COLUMN sys."user".status IS '状态：1-启用 2-禁用';
COMMENT ON COLUMN sys."user".create_by IS '创建人';
COMMENT ON COLUMN sys."user".create_time IS '创建时间';
COMMENT ON COLUMN sys."user".update_by IS '更新人';
COMMENT ON COLUMN sys."user".update_time IS '更新时间';

-- ============================================================
-- 测试数据（密码为 123456 的 bcrypt 哈希）
-- ============================================================

INSERT INTO sys."user" (user_name, password, nick_name, gender, email, status, create_by)
VALUES
    ('Super',  '$2a$10$N.zmdr9k7uOCQb376NoUnuTJ8iAt6Z2Rx1z4TqL9Z0.Dq3GwLFpK6', '超级管理员', 1, 'super@example.com',  1, 'system'),
    ('Admin',  '$2a$10$N.zmdr9k7uOCQb376NoUnuTJ8iAt6Z2Rx1z4TqL9Z0.Dq3GwLFpK6', '管理员',     1, 'admin@example.com',  1, 'system'),
    ('User',   '$2a$10$N.zmdr9k7uOCQb376NoUnuTJ8iAt6Z2Rx1z4TqL9Z0.Dq3GwLFpK6', '普通用户',   1, 'user@example.com',   1, 'system');
