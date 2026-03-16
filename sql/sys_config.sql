-- ============================================================
-- 系统参数配置表
-- ============================================================

CREATE TABLE sys_config (
    id            BIGSERIAL       PRIMARY KEY,
    config_name   VARCHAR(100)    NOT NULL,
    config_key    VARCHAR(100)    NOT NULL,
    config_value  TEXT            NOT NULL DEFAULT '',
    default_value TEXT            NOT NULL DEFAULT '',
    value_type    SMALLINT        NOT NULL DEFAULT 1,
    config_group  VARCHAR(64)     NOT NULL DEFAULT '',
    option_dict_type VARCHAR(100) NOT NULL DEFAULT '',
    config_sort   INT             NOT NULL DEFAULT 0,
    enabled       BOOLEAN         NOT NULL DEFAULT TRUE,
    is_system     BOOLEAN         NOT NULL DEFAULT FALSE,
    remark        VARCHAR(500)    NOT NULL DEFAULT '',
    create_by     VARCHAR(64)     NOT NULL DEFAULT '',
    create_time   TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by     VARCHAR(64)     NOT NULL DEFAULT '',
    update_time   TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_config_config_key ON sys_config (config_key);
CREATE INDEX idx_sys_config_group_sort ON sys_config (config_group, config_sort);
CREATE INDEX idx_sys_config_enabled ON sys_config (enabled);

COMMENT ON TABLE sys_config IS '系统参数配置表';
COMMENT ON COLUMN sys_config.id IS '配置ID';
COMMENT ON COLUMN sys_config.config_name IS '配置名称';
COMMENT ON COLUMN sys_config.config_key IS '配置键（唯一标识，如 sys.site.name）';
COMMENT ON COLUMN sys_config.config_value IS '当前配置值，统一按字符串存储，按 value_type 解析';
COMMENT ON COLUMN sys_config.default_value IS '默认配置值，用于重置或回退';
COMMENT ON COLUMN sys_config.value_type IS '值类型：1=文本 2=数字 3=布尔 4=文本域 5=下拉单选 6=JSON 7=密码 8=图片';
COMMENT ON COLUMN sys_config.config_group IS '配置分组编码（如 basic/upload/security，后续可结合字典维护）';
COMMENT ON COLUMN sys_config.option_dict_type IS '候选项字典类型编码，当 value_type=5 时使用，对应 sys_dict_type.dict_type';
COMMENT ON COLUMN sys_config.config_sort IS '同分组内排序，值越小越靠前';
COMMENT ON COLUMN sys_config.enabled IS '是否启用';
COMMENT ON COLUMN sys_config.is_system IS '是否系统内置（防止误删）';
COMMENT ON COLUMN sys_config.remark IS '备注';
COMMENT ON COLUMN sys_config.create_by IS '创建人';
COMMENT ON COLUMN sys_config.create_time IS '创建时间';
COMMENT ON COLUMN sys_config.update_by IS '更新人';
COMMENT ON COLUMN sys_config.update_time IS '更新时间';

-- ============================================================
-- 初始化基础配置
-- ============================================================

-- 基础设置
INSERT INTO sys_config (
    config_name, config_key, config_value, default_value, value_type,
    config_group, config_sort, enabled, is_system, remark, create_by
) VALUES
    ('站点名称', 'sys.site.name', 'Summer Admin', 'Summer Admin', 1,
     'basic', 1, TRUE, TRUE, '系统显示的站点名称', 'system'),
    ('站点 Logo', 'sys.site.logo', '', '', 8,
     'basic', 2, TRUE, TRUE, '站点 Logo 图片地址', 'system'),
    ('版权信息', 'sys.site.copyright', 'Copyright © 2026 Summer', 'Copyright © 2026 Summer', 1,
     'basic', 3, TRUE, TRUE, '页面底部版权文案', 'system');

-- 上传设置
INSERT INTO sys_config (
    config_name, config_key, config_value, default_value, value_type,
    config_group, config_sort, enabled, is_system, remark, create_by
) VALUES
    ('单文件上传大小限制', 'sys.upload.max_size_mb', '10', '10', 2,
     'upload', 1, TRUE, TRUE, '单位 MB', 'system'),
    ('允许上传文件后缀', 'sys.upload.allowed_exts', 'jpg,jpeg,png,gif,webp,pdf,doc,docx,xls,xlsx', 'jpg,jpeg,png,gif,webp,pdf,doc,docx,xls,xlsx', 4,
     'upload', 2, TRUE, TRUE, '多个后缀用英文逗号分隔', 'system');

-- 安全设置
INSERT INTO sys_config (
    config_name, config_key, config_value, default_value, value_type,
    config_group, config_sort, enabled, is_system, remark, create_by
) VALUES
    ('登录验证码开关', 'sys.security.captcha_enabled', 'true', 'true', 3,
     'security', 1, TRUE, TRUE, '登录页是否启用验证码', 'system'),
    ('用户初始密码', 'sys.user.init_password', '123456', '123456', 7,
     'security', 2, TRUE, TRUE, '后台创建用户时使用的初始密码', 'system'),
    ('开放注册开关', 'sys.user.register_enabled', 'false', 'false', 3,
     'security', 3, TRUE, TRUE, '是否允许新用户自主注册', 'system');
