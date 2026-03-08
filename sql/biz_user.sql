-- B 端用户表
CREATE TABLE biz_user (
    id          BIGSERIAL    PRIMARY KEY,
    user_name   VARCHAR(64)  NOT NULL UNIQUE,
    password    VARCHAR(256) NOT NULL,
    nick_name   VARCHAR(64)  NOT NULL DEFAULT '',
    phone       VARCHAR(32)  NOT NULL DEFAULT '',
    email       VARCHAR(128) NOT NULL DEFAULT '',
    avatar      VARCHAR(512) NOT NULL DEFAULT '',
    status      SMALLINT     NOT NULL DEFAULT 1,  -- 1:启用 2:禁用 3:注销
    create_by   VARCHAR(64)  NOT NULL DEFAULT '',
    create_time TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by   VARCHAR(64)  NOT NULL DEFAULT '',
    update_time TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE biz_user IS 'B 端用户表';
COMMENT ON COLUMN biz_user.status IS '账号状态: 1-启用 2-禁用 3-注销';
