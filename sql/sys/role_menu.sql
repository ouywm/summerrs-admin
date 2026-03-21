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
-- 测试数据
-- R_SUPER(1): 拥有所有菜单和按钮
-- R_ADMIN(2): 拥有所有菜单和按钮（不含菜单管理）
-- R_USER(3):  仅仪表盘
-- ============================================================

-- R_SUPER → 全部
INSERT INTO sys.role_menu (role_id, menu_id)
VALUES
    (1, 1), (1, 2), (1, 3),
    (1, 4), (1, 5), (1, 6), (1, 7),
    (1, 8), (1, 9), (1, 10), (1, 11);

-- R_ADMIN → 仪表盘 + 系统管理（不含菜单管理）+ 用户按钮
INSERT INTO sys.role_menu (role_id, menu_id)
VALUES
    (2, 1), (2, 2), (2, 3),
    (2, 4), (2, 5), (2, 6),
    (2, 8), (2, 9), (2, 10), (2, 11);

-- R_USER → 仅仪表盘
INSERT INTO sys.role_menu (role_id, menu_id)
VALUES
    (3, 1), (3, 2), (3, 3);