-- ============================================================
-- 系统参数分组表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.config_group (
    id          BIGSERIAL     PRIMARY KEY,
    group_name  VARCHAR(100)  NOT NULL,
    group_code  VARCHAR(64)   NOT NULL,
    group_sort  INT           NOT NULL DEFAULT 0,
    enabled     BOOLEAN       NOT NULL DEFAULT TRUE,
    is_system   BOOLEAN       NOT NULL DEFAULT FALSE,
    remark      VARCHAR(500)  NOT NULL DEFAULT '',
    create_by   VARCHAR(64)   NOT NULL DEFAULT '',
    create_time TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by   VARCHAR(64)   NOT NULL DEFAULT '',
    update_time TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_config_group_group_code ON sys.config_group (group_code);
CREATE INDEX idx_sys_config_group_sort ON sys.config_group (group_sort);
CREATE INDEX idx_sys_config_group_enabled ON sys.config_group (enabled);

COMMENT ON TABLE sys.config_group IS '系统参数分组表';
COMMENT ON COLUMN sys.config_group.id IS '分组ID';
COMMENT ON COLUMN sys.config_group.group_name IS '分组名称';
COMMENT ON COLUMN sys.config_group.group_code IS '分组编码（唯一标识，如 basic/security）';
COMMENT ON COLUMN sys.config_group.group_sort IS '分组排序，值越小越靠前';
COMMENT ON COLUMN sys.config_group.enabled IS '是否启用';
COMMENT ON COLUMN sys.config_group.is_system IS '是否系统内置（防止误删）';
COMMENT ON COLUMN sys.config_group.remark IS '备注';
COMMENT ON COLUMN sys.config_group.create_by IS '创建人';
COMMENT ON COLUMN sys.config_group.create_time IS '创建时间';
COMMENT ON COLUMN sys.config_group.update_by IS '更新人';
COMMENT ON COLUMN sys.config_group.update_time IS '更新时间';

-- ============================================================
-- 系统参数配置表
-- ============================================================

CREATE TABLE sys.config (
    id              BIGSERIAL       PRIMARY KEY,
    config_name     VARCHAR(100)    NOT NULL,
    config_key      VARCHAR(100)    NOT NULL,
    config_value    TEXT            NOT NULL DEFAULT '',
    default_value   TEXT            NOT NULL DEFAULT '',
    value_type      SMALLINT        NOT NULL DEFAULT 1,
    config_group_id BIGINT          NOT NULL DEFAULT 0,
    option_dict_type VARCHAR(100) NOT NULL DEFAULT '',
    config_sort     INT             NOT NULL DEFAULT 0,
    enabled         BOOLEAN         NOT NULL DEFAULT TRUE,
    is_system       BOOLEAN         NOT NULL DEFAULT FALSE,
    remark          VARCHAR(500)    NOT NULL DEFAULT '',
    create_by       VARCHAR(64)     NOT NULL DEFAULT '',
    create_time     TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by       VARCHAR(64)     NOT NULL DEFAULT '',
    update_time     TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_config_config_key ON sys.config (config_key);
CREATE INDEX idx_sys_config_group_id_sort ON sys.config (config_group_id, config_sort);
CREATE INDEX idx_sys_config_enabled ON sys.config (enabled);

COMMENT ON TABLE sys.config IS '系统参数配置表';
COMMENT ON COLUMN sys.config.id IS '配置ID';
COMMENT ON COLUMN sys.config.config_name IS '配置名称';
COMMENT ON COLUMN sys.config.config_key IS '配置键（唯一标识，如 sys.site.name）';
COMMENT ON COLUMN sys.config.config_value IS '当前配置值，统一按字符串存储，按 value_type 解析';
COMMENT ON COLUMN sys.config.default_value IS '默认配置值，用于重置或回退';
COMMENT ON COLUMN sys.config.value_type IS '值类型：1=文本 2=数字 3=布尔 4=文本域 5=下拉单选 6=JSON 7=密码 8=图片';
COMMENT ON COLUMN sys.config.config_group_id IS '配置分组ID，逻辑关联 sys.config_group.id';
COMMENT ON COLUMN sys.config.option_dict_type IS '候选项字典类型编码，当 value_type=5 时使用，对应 sys.dict_type.dict_type';
COMMENT ON COLUMN sys.config.config_sort IS '同分组内排序，值越小越靠前';
COMMENT ON COLUMN sys.config.enabled IS '是否启用';
COMMENT ON COLUMN sys.config.is_system IS '是否系统内置（防止误删）';
COMMENT ON COLUMN sys.config.remark IS '备注';
COMMENT ON COLUMN sys.config.create_by IS '创建人';
COMMENT ON COLUMN sys.config.create_time IS '创建时间';
COMMENT ON COLUMN sys.config.update_by IS '更新人';
COMMENT ON COLUMN sys.config.update_time IS '更新时间';
