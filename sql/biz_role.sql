-- B 端角色表
CREATE TABLE biz_role (
    id          BIGSERIAL    PRIMARY KEY,
    role_name   VARCHAR(64)  NOT NULL,
    role_code   VARCHAR(64)  NOT NULL UNIQUE,
    status      SMALLINT     NOT NULL DEFAULT 1,  -- 1:启用 2:禁用
    remark      VARCHAR(256) NOT NULL DEFAULT '',
    create_by   VARCHAR(64)  NOT NULL DEFAULT '',
    create_time TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by   VARCHAR(64)  NOT NULL DEFAULT '',
    update_time TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- B 端用户角色关联表
CREATE TABLE biz_user_role (
    id       BIGSERIAL PRIMARY KEY,
    user_id  BIGINT NOT NULL REFERENCES biz_user(id),
    role_id  BIGINT NOT NULL REFERENCES biz_role(id),
    UNIQUE(user_id, role_id)
);

COMMENT ON TABLE biz_role IS 'B 端角色表';
COMMENT ON TABLE biz_user_role IS 'B 端用户角色关联表';
