-- 用户登录日志表
CREATE TABLE sys_login_log
(
    id             BIGSERIAL PRIMARY KEY,
    user_id        BIGSERIAL NOT NULL,
    user_name      VARCHAR(50)  NOT NULL,
    login_time     TIMESTAMP    NOT NULL,
    login_ip       INET         NOT NULL,
    login_location VARCHAR(200),
    user_agent     TEXT,
    browser        VARCHAR(100),
    browser_version VARCHAR(50),
    os             VARCHAR(100),
    os_version     VARCHAR(50),
    device         VARCHAR(50),
    status         SMALLINT     NOT NULL,
    fail_reason    VARCHAR(200),
    create_time    TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 创建索引
CREATE INDEX idx_sys_login_log_user_id ON sys_login_log (user_id);
CREATE INDEX idx_sys_login_log_login_time ON sys_login_log (login_time);
CREATE INDEX idx_sys_login_log_status ON sys_login_log (status);

-- 添加注释
COMMENT ON TABLE sys_login_log IS '用户登录日志表';
COMMENT ON COLUMN sys_login_log.id IS '主键ID';
COMMENT ON COLUMN sys_login_log.user_id IS '用户ID';
COMMENT ON COLUMN sys_login_log.user_name IS '用户名';
COMMENT ON COLUMN sys_login_log.login_time IS '登录时间';
COMMENT ON COLUMN sys_login_log.login_ip IS '登录IP';
COMMENT ON COLUMN sys_login_log.login_location IS '登录地理位置';
COMMENT ON COLUMN sys_login_log.user_agent IS '浏览器User-Agent';
COMMENT ON COLUMN sys_login_log.browser IS '浏览器';
COMMENT ON COLUMN sys_login_log.browser_version IS '浏览器版本';
COMMENT ON COLUMN sys_login_log.os IS '操作系统';
COMMENT ON COLUMN sys_login_log.os_version IS '操作系统版本';
COMMENT ON COLUMN sys_login_log.device IS '设备类型';
COMMENT ON COLUMN sys_login_log.status IS '登录状态（1=成功, 2=失败）';
COMMENT ON COLUMN sys_login_log.fail_reason IS '失败原因';
COMMENT ON COLUMN sys_login_log.create_time IS '创建时间';
