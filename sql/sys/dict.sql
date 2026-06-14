-- ============================================================
-- 字典类型表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.dict_type (
    id          BIGSERIAL       PRIMARY KEY,
    dict_name   VARCHAR(100)    NOT NULL,
    dict_type   VARCHAR(100)    NOT NULL,
    status      SMALLINT        NOT NULL DEFAULT 1,
    is_system   BOOLEAN         NOT NULL DEFAULT false,
    remark      VARCHAR(500)    NOT NULL DEFAULT '',
    create_by   VARCHAR(64)     NOT NULL DEFAULT '',
    create_time TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by   VARCHAR(64)     NOT NULL DEFAULT '',
    update_time TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_dict_type_dict_type ON sys.dict_type (dict_type);

COMMENT ON TABLE sys.dict_type IS '字典类型表';
COMMENT ON COLUMN sys.dict_type.id IS '字典类型ID';
COMMENT ON COLUMN sys.dict_type.dict_name IS '字典名称';
COMMENT ON COLUMN sys.dict_type.dict_type IS '字典类型编码（唯一）';
COMMENT ON COLUMN sys.dict_type.status IS '状态：1-启用 2-禁用';
COMMENT ON COLUMN sys.dict_type.is_system IS '是否系统内置（防止误删）';
COMMENT ON COLUMN sys.dict_type.remark IS '备注';
COMMENT ON COLUMN sys.dict_type.create_by IS '创建人';
COMMENT ON COLUMN sys.dict_type.create_time IS '创建时间';
COMMENT ON COLUMN sys.dict_type.update_by IS '更新人';
COMMENT ON COLUMN sys.dict_type.update_time IS '更新时间';

-- ============================================================
-- 字典数据表
-- ============================================================

CREATE TABLE sys.dict_data (
    id          BIGSERIAL       PRIMARY KEY,
    dict_type   VARCHAR(100)    NOT NULL,
    dict_label  VARCHAR(100)    NOT NULL,
    dict_value  VARCHAR(100)    NOT NULL,
    dict_sort   INT             NOT NULL DEFAULT 0,
    css_class   VARCHAR(100)    NOT NULL DEFAULT '',
    list_class  VARCHAR(100)    NOT NULL DEFAULT '',
    is_default  BOOLEAN         NOT NULL DEFAULT false,
    status      SMALLINT        NOT NULL DEFAULT 1,
    is_system   BOOLEAN         NOT NULL DEFAULT false,
    remark      VARCHAR(500)    NOT NULL DEFAULT '',
    create_by   VARCHAR(64)     NOT NULL DEFAULT '',
    create_time TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by   VARCHAR(64)     NOT NULL DEFAULT '',
    update_time TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sys_dict_data_dict_type ON sys.dict_data (dict_type);

COMMENT ON TABLE sys.dict_data IS '字典数据表';
COMMENT ON COLUMN sys.dict_data.id IS '字典数据ID';
COMMENT ON COLUMN sys.dict_data.dict_type IS '字典类型编码';
COMMENT ON COLUMN sys.dict_data.dict_label IS '字典标签（显示值）';
COMMENT ON COLUMN sys.dict_data.dict_value IS '字典键值（实际值）';
COMMENT ON COLUMN sys.dict_data.dict_sort IS '排序';
COMMENT ON COLUMN sys.dict_data.css_class IS 'CSS类名';
COMMENT ON COLUMN sys.dict_data.list_class IS '列表样式（primary/success/warning/danger/info）';
COMMENT ON COLUMN sys.dict_data.is_default IS '是否默认选项';
COMMENT ON COLUMN sys.dict_data.status IS '状态：1-启用 2-禁用';
COMMENT ON COLUMN sys.dict_data.is_system IS '是否系统内置（防止误删）';
COMMENT ON COLUMN sys.dict_data.remark IS '备注';
COMMENT ON COLUMN sys.dict_data.create_by IS '创建人';
COMMENT ON COLUMN sys.dict_data.create_time IS '创建时间';
COMMENT ON COLUMN sys.dict_data.update_by IS '更新人';
COMMENT ON COLUMN sys.dict_data.update_time IS '更新时间';
