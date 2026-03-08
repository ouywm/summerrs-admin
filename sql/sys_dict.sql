-- ============================================================
-- 字典类型表
-- ============================================================

CREATE TABLE sys_dict_type (
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

CREATE UNIQUE INDEX uk_sys_dict_type_dict_type ON sys_dict_type (dict_type);

COMMENT ON TABLE sys_dict_type IS '字典类型表';
COMMENT ON COLUMN sys_dict_type.id IS '字典类型ID';
COMMENT ON COLUMN sys_dict_type.dict_name IS '字典名称';
COMMENT ON COLUMN sys_dict_type.dict_type IS '字典类型编码（唯一）';
COMMENT ON COLUMN sys_dict_type.status IS '状态：1-启用 2-禁用';
COMMENT ON COLUMN sys_dict_type.is_system IS '是否系统内置（防止误删）';
COMMENT ON COLUMN sys_dict_type.remark IS '备注';
COMMENT ON COLUMN sys_dict_type.create_by IS '创建人';
COMMENT ON COLUMN sys_dict_type.create_time IS '创建时间';
COMMENT ON COLUMN sys_dict_type.update_by IS '更新人';
COMMENT ON COLUMN sys_dict_type.update_time IS '更新时间';

-- ============================================================
-- 字典数据表
-- ============================================================

CREATE TABLE sys_dict_data (
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

CREATE INDEX idx_sys_dict_data_dict_type ON sys_dict_data (dict_type);

COMMENT ON TABLE sys_dict_data IS '字典数据表';
COMMENT ON COLUMN sys_dict_data.id IS '字典数据ID';
COMMENT ON COLUMN sys_dict_data.dict_type IS '字典类型编码';
COMMENT ON COLUMN sys_dict_data.dict_label IS '字典标签（显示值）';
COMMENT ON COLUMN sys_dict_data.dict_value IS '字典键值（实际值）';
COMMENT ON COLUMN sys_dict_data.dict_sort IS '排序';
COMMENT ON COLUMN sys_dict_data.css_class IS 'CSS类名';
COMMENT ON COLUMN sys_dict_data.list_class IS '列表样式（primary/success/warning/danger/info）';
COMMENT ON COLUMN sys_dict_data.is_default IS '是否默认选项';
COMMENT ON COLUMN sys_dict_data.status IS '状态：1-启用 2-禁用';
COMMENT ON COLUMN sys_dict_data.is_system IS '是否系统内置（防止误删）';
COMMENT ON COLUMN sys_dict_data.remark IS '备注';
COMMENT ON COLUMN sys_dict_data.create_by IS '创建人';
COMMENT ON COLUMN sys_dict_data.create_time IS '创建时间';
COMMENT ON COLUMN sys_dict_data.update_by IS '更新人';
COMMENT ON COLUMN sys_dict_data.update_time IS '更新时间';

-- ============================================================
-- 初始化字典类型
-- ============================================================

INSERT INTO sys_dict_type (dict_name, dict_type, status, is_system, remark, create_by) VALUES
('用户状态', 'user_status', 1, true, '用户账号状态', 'system'),
('用户性别', 'user_gender', 1, true, '用户性别选项', 'system'),
('系统状态', 'sys_status', 1, true, '通用启用禁用状态', 'system'),
('菜单类型', 'menu_type', 1, true, '菜单和按钮类型', 'system'),
('业务类型', 'business_type', 1, true, '操作日志业务类型', 'system'),
('登录状态', 'login_status', 1, true, '登录日志状态', 'system');

-- ============================================================
-- 初始化字典数据
-- ============================================================

-- 用户状态
INSERT INTO sys_dict_data (dict_type, dict_label, dict_value, dict_sort, list_class, is_system, create_by) VALUES
('user_status', '启用', '1', 1, 'success', true, 'system'),
('user_status', '禁用', '2', 2, 'danger', true, 'system'),
('user_status', '注销', '3', 3, 'info', true, 'system');

-- 用户性别
INSERT INTO sys_dict_data (dict_type, dict_label, dict_value, dict_sort, list_class, is_system, create_by) VALUES
('user_gender', '未知', '0', 1, 'info', true, 'system'),
('user_gender', '男', '1', 2, 'primary', true, 'system'),
('user_gender', '女', '2', 3, 'danger', true, 'system');

-- 系统状态
INSERT INTO sys_dict_data (dict_type, dict_label, dict_value, dict_sort, list_class, is_system, create_by) VALUES
('sys_status', '启用', '1', 1, 'success', true, 'system'),
('sys_status', '禁用', '2', 2, 'danger', true, 'system');

-- 菜单类型
INSERT INTO sys_dict_data (dict_type, dict_label, dict_value, dict_sort, list_class, is_system, create_by) VALUES
('menu_type', '菜单', '1', 1, 'primary', true, 'system'),
('menu_type', '按钮', '2', 2, 'success', true, 'system');

-- 业务类型
INSERT INTO sys_dict_data (dict_type, dict_label, dict_value, dict_sort, list_class, is_system, create_by) VALUES
('business_type', '其他', '0', 1, 'info', true, 'system'),
('business_type', '新增', '1', 2, 'success', true, 'system'),
('business_type', '修改', '2', 3, 'primary', true, 'system'),
('business_type', '删除', '3', 4, 'danger', true, 'system'),
('business_type', '查询', '4', 5, 'info', true, 'system'),
('business_type', '导出', '5', 6, 'warning', true, 'system'),
('business_type', '导入', '6', 7, 'warning', true, 'system'),
('business_type', '认证', '7', 8, 'primary', true, 'system');

-- 登录状态
INSERT INTO sys_dict_data (dict_type, dict_label, dict_value, dict_sort, list_class, is_system, create_by) VALUES
('login_status', '成功', '1', 1, 'success', true, 'system'),
('login_status', '失败', '2', 2, 'danger', true, 'system');