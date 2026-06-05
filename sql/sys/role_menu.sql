-- ============================================================
-- 角色菜单关联表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.role_menu (
    id      BIGSERIAL   PRIMARY KEY,
    role_id BIGINT      NOT NULL,
    menu_id BIGINT      NOT NULL
);

CREATE UNIQUE INDEX uk_sys_role_menu ON sys.role_menu (role_id, menu_id);
CREATE INDEX idx_sys_role_menu_role_id ON sys.role_menu (role_id);
CREATE INDEX idx_sys_role_menu_menu_id ON sys.role_menu (menu_id);

COMMENT ON TABLE sys.role_menu IS '角色菜单关联表';
COMMENT ON COLUMN sys.role_menu.id IS '主键ID';
COMMENT ON COLUMN sys.role_menu.role_id IS '角色ID（关联 sys."role".id）';
COMMENT ON COLUMN sys.role_menu.menu_id IS '菜单ID（关联 sys.menu.id）';

-- ============================================================
-- 初始化数据（依赖 sql/sys/role.sql 与 sql/sys/menu_data_all.sql 先执行）
--
-- 用 INSERT ... SELECT 从 sys.menu 动态取菜单 ID，不写死编号：
-- 菜单增删后重跑本段即可，授权关系自动对齐当前菜单全集。
--   R_SUPER：全部菜单与按钮
--   R_ADMIN：全部，但排除"菜单管理"(Menus)及其按钮
--   R_USER ：仅仪表盘(Dashboard)及其子页
-- ============================================================

TRUNCATE TABLE sys.role_menu RESTART IDENTITY;

-- R_SUPER → 全部菜单
INSERT INTO sys.role_menu (role_id, menu_id)
SELECT r.id, m.id
FROM sys."role" r
CROSS JOIN sys.menu m
WHERE r.role_code = 'R_SUPER';

-- R_ADMIN → 全部菜单，排除"菜单管理"(Menus)及其下挂按钮
INSERT INTO sys.role_menu (role_id, menu_id)
SELECT r.id, m.id
FROM sys."role" r
CROSS JOIN sys.menu m
WHERE r.role_code = 'R_ADMIN'
  AND m.id NOT IN (SELECT id FROM sys.menu WHERE name = 'Menus')
  AND m.parent_id NOT IN (SELECT id FROM sys.menu WHERE name = 'Menus');

-- R_USER → 仅仪表盘（根节点 Dashboard 及其直接子页）
INSERT INTO sys.role_menu (role_id, menu_id)
SELECT r.id, m.id
FROM sys."role" r
CROSS JOIN sys.menu m
WHERE r.role_code = 'R_USER'
  AND (m.name = 'Dashboard'
       OR m.parent_id IN (SELECT id FROM sys.menu WHERE name = 'Dashboard'));
