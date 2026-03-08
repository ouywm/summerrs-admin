-- C 端用户表
CREATE TABLE customer (
    id          BIGSERIAL    PRIMARY KEY,
    phone       VARCHAR(32)  NOT NULL UNIQUE,  -- 手机号作为登录标识
    password    VARCHAR(256) NOT NULL,
    nick_name   VARCHAR(64)  NOT NULL DEFAULT '',
    avatar      VARCHAR(512) NOT NULL DEFAULT '',
    status      SMALLINT     NOT NULL DEFAULT 1,  -- 1:启用 2:禁用 3:注销
    create_time TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE customer IS 'C 端用户表';
COMMENT ON COLUMN customer.phone IS '手机号（登录标识）';
COMMENT ON COLUMN customer.status IS '账号状态: 1-启用 2-禁用 3-注销';
