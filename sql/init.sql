-- sys_user 系统用户表
CREATE TABLE sys_user
(
    id         BIGSERIAL    PRIMARY KEY,
    username   VARCHAR(64)  NOT NULL UNIQUE,
    password   VARCHAR(256) NOT NULL,
    nickname   VARCHAR(64)  NOT NULL DEFAULT '',
    email      VARCHAR(128),
    phone      VARCHAR(32),
    avatar     VARCHAR(512),
    status     SMALLINT     NOT NULL DEFAULT 1,
    created_at TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE  sys_user             IS '系统用户表';
COMMENT ON COLUMN sys_user.username    IS '用户名';
COMMENT ON COLUMN sys_user.password    IS '密码（加密存储）';
COMMENT ON COLUMN sys_user.nickname    IS '昵称';
COMMENT ON COLUMN sys_user.email       IS '邮箱';
COMMENT ON COLUMN sys_user.phone       IS '手机号';
COMMENT ON COLUMN sys_user.avatar      IS '头像地址';
COMMENT ON COLUMN sys_user.status      IS '状态: 1=启用, 0=禁用';
COMMENT ON COLUMN sys_user.created_at  IS '创建时间';
COMMENT ON COLUMN sys_user.updated_at  IS '更新时间';
