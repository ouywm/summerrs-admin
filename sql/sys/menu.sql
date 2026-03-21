-- ============================================================
-- 系统菜单表（菜单 + 按钮权限共用）
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.menu (
    id              BIGSERIAL       PRIMARY KEY,
    parent_id       BIGINT          NOT NULL DEFAULT 0,
    menu_type       SMALLINT        NOT NULL DEFAULT 1,
    name            VARCHAR(64)     NOT NULL DEFAULT '',
    path            VARCHAR(256)    NOT NULL DEFAULT '',
    component       VARCHAR(256)    NOT NULL DEFAULT '',
    redirect        VARCHAR(256)    NOT NULL DEFAULT '',
    icon            VARCHAR(64)     NOT NULL DEFAULT '',
    title           VARCHAR(64)     NOT NULL,
    link            VARCHAR(512)    NOT NULL DEFAULT '',
    is_iframe       BOOLEAN         NOT NULL DEFAULT FALSE,
    is_hide         BOOLEAN         NOT NULL DEFAULT FALSE,
    is_hide_tab     BOOLEAN         NOT NULL DEFAULT FALSE,
    is_full_page    BOOLEAN         NOT NULL DEFAULT FALSE,
    is_first_level  BOOLEAN         NOT NULL DEFAULT FALSE,
    keep_alive      BOOLEAN         NOT NULL DEFAULT FALSE,
    fixed_tab       BOOLEAN         NOT NULL DEFAULT FALSE,
    show_badge      BOOLEAN         NOT NULL DEFAULT FALSE,
    show_text_badge VARCHAR(32)     NOT NULL DEFAULT '',
    active_path     VARCHAR(256)    NOT NULL DEFAULT '',
    auth_name       VARCHAR(64)     NOT NULL DEFAULT '',
    auth_mark       VARCHAR(64)     NOT NULL DEFAULT '',
    sort            INT             NOT NULL DEFAULT 0,
    enabled         BOOLEAN         NOT NULL DEFAULT TRUE,
    create_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sys_menu_parent_id ON sys.menu (parent_id);

COMMENT ON TABLE sys.menu IS '系统菜单表（菜单与按钮权限共用）';
COMMENT ON COLUMN sys.menu.id IS '菜单ID';
COMMENT ON COLUMN sys.menu.parent_id IS '父级菜单ID（0表示一级菜单）';
COMMENT ON COLUMN sys.menu.menu_type IS '类型：1-菜单 2-按钮权限';
COMMENT ON COLUMN sys.menu.name IS '路由名称（唯一标识，如 SystemUser）';
COMMENT ON COLUMN sys.menu.path IS '路由路径（一级以/开头，子级不以/开头）';
COMMENT ON COLUMN sys.menu.component IS '组件路径（相对于 src/views，如 system/user/index）';
COMMENT ON COLUMN sys.menu.redirect IS '重定向路径';
COMMENT ON COLUMN sys.menu.icon IS '菜单图标';
COMMENT ON COLUMN sys.menu.title IS '菜单标题';
COMMENT ON COLUMN sys.menu.link IS '外部链接URL';
COMMENT ON COLUMN sys.menu.is_iframe IS '是否为iframe内嵌';
COMMENT ON COLUMN sys.menu.is_hide IS '是否在菜单中隐藏';
COMMENT ON COLUMN sys.menu.is_hide_tab IS '是否在标签页中隐藏';
COMMENT ON COLUMN sys.menu.is_full_page IS '是否全屏页面';
COMMENT ON COLUMN sys.menu.is_first_level IS '是否为一级菜单（无子菜单的独立页面）';
COMMENT ON COLUMN sys.menu.keep_alive IS '是否缓存页面';
COMMENT ON COLUMN sys.menu.fixed_tab IS '是否固定标签页';
COMMENT ON COLUMN sys.menu.show_badge IS '是否显示徽章';
COMMENT ON COLUMN sys.menu.show_text_badge IS '文本徽章内容';
COMMENT ON COLUMN sys.menu.active_path IS '高亮的菜单路径';
COMMENT ON COLUMN sys.menu.auth_name IS '按钮权限名称（menu_type=2时使用）';
COMMENT ON COLUMN sys.menu.auth_mark IS '按钮权限标识（menu_type=2时使用，如 btn_add）';
COMMENT ON COLUMN sys.menu.sort IS '排序（数值越小越靠前）';
COMMENT ON COLUMN sys.menu.enabled IS '是否启用';
COMMENT ON COLUMN sys.menu.create_time IS '创建时间';
COMMENT ON COLUMN sys.menu.update_time IS '更新时间';

-- ============================================================
-- 测试数据
-- ============================================================

-- 一级菜单：仪表盘
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, redirect, icon, title, sort)
VALUES (1, 0, 1, 'Dashboard', '/dashboard', '/dashboard/console', 'dashboard', '仪表盘', 1);

-- 二级菜单：控制台、分析页
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, title, sort)
VALUES
    (2, 1, 1, 'DashboardConsole',  'console',  'dashboard/console/index',  '控制台', 1),
    (3, 1, 1, 'DashboardAnalysis', 'analysis', 'dashboard/analysis/index', '分析页', 2);

-- 一级菜单：系统管理
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, redirect, icon, title, sort)
VALUES (4, 0, 1, 'System', '/system', '/system/user', 'system', '系统管理', 2);

-- 二级菜单：用户管理、角色管理、菜单管理
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, title, sort)
VALUES
    (5, 4, 1, 'SystemUser', 'user', 'system/user/index', '用户管理', 1),
    (6, 4, 1, 'SystemRole', 'role', 'system/role/index', '角色管理', 2),
    (7, 4, 1, 'SystemMenu', 'menu', 'system/menu/index', '菜单管理', 3);

-- 按钮权限：用户管理下的操作按钮
INSERT INTO sys.menu (id, parent_id, menu_type, title, auth_name, auth_mark, sort)
VALUES
    (8,  5, 2, '新增用户', '新增', 'btn_add',    1),
    (9,  5, 2, '编辑用户', '编辑', 'btn_edit',   2),
    (10, 5, 2, '删除用户', '删除', 'btn_delete', 3),
    (11, 5, 2, '导出用户', '导出', 'btn_export', 4);

-- 重置序列
SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu));