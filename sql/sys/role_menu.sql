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
