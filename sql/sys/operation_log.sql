-- 系统操作日志表
CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.operation_log
(
    id              BIGSERIAL PRIMARY KEY,
    user_id         BIGINT,
    user_name       VARCHAR(64),
    module          VARCHAR(64)   NOT NULL,
    action          VARCHAR(128)  NOT NULL,
    business_type   SMALLINT      NOT NULL DEFAULT 0,
    request_method  VARCHAR(10)   NOT NULL,
    request_url     VARCHAR(512)  NOT NULL,
    request_params  JSONB,
    response_body   JSONB,
    response_code   SMALLINT      NOT NULL DEFAULT 200,
    client_ip       INET,
    ip_location     VARCHAR(200),
    user_agent      TEXT,
    status          SMALLINT      NOT NULL DEFAULT 1,
    error_msg       TEXT,
    duration        BIGINT        NOT NULL DEFAULT 0,
    create_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 创建索引
CREATE INDEX idx_sys_operation_log_user_id ON sys.operation_log (user_id);
CREATE INDEX idx_sys_operation_log_module ON sys.operation_log (module);
CREATE INDEX idx_sys_operation_log_business_type ON sys.operation_log (business_type);
CREATE INDEX idx_sys_operation_log_status ON sys.operation_log (status);
CREATE INDEX idx_sys_operation_log_create_time ON sys.operation_log (create_time);

-- 添加注释
COMMENT ON TABLE sys.operation_log IS '系统操作日志表';
COMMENT ON COLUMN sys.operation_log.id IS '主键ID';
COMMENT ON COLUMN sys.operation_log.user_id IS '操作人ID';
COMMENT ON COLUMN sys.operation_log.user_name IS '操作人用户名';
COMMENT ON COLUMN sys.operation_log.module IS '业务模块';
COMMENT ON COLUMN sys.operation_log.action IS '操作描述';
COMMENT ON COLUMN sys.operation_log.business_type IS '操作类型（0=其他, 1=新增, 2=修改, 3=删除, 4=查询, 5=导出, 6=导入, 7=授权）';
COMMENT ON COLUMN sys.operation_log.request_method IS 'HTTP请求方法';
COMMENT ON COLUMN sys.operation_log.request_url IS '请求URL';
COMMENT ON COLUMN sys.operation_log.request_params IS '请求参数（JSON，敏感接口不记录）';
COMMENT ON COLUMN sys.operation_log.response_body IS '响应内容（JSON）';
COMMENT ON COLUMN sys.operation_log.response_code IS 'HTTP响应状态码';
COMMENT ON COLUMN sys.operation_log.client_ip IS '客户端IP';
COMMENT ON COLUMN sys.operation_log.ip_location IS 'IP地理位置';
COMMENT ON COLUMN sys.operation_log.user_agent IS '浏览器User-Agent';
COMMENT ON COLUMN sys.operation_log.status IS '操作状态（1=成功, 2=失败, 3=异常）';
COMMENT ON COLUMN sys.operation_log.error_msg IS '错误信息';
COMMENT ON COLUMN sys.operation_log.duration IS '耗时（毫秒）';
COMMENT ON COLUMN sys.operation_log.create_time IS '操作时间';