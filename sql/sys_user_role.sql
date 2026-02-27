-- ============================================================
-- 用户角色关联表
-- ============================================================

CREATE TABLE sys_user_role (
    id      BIGSERIAL   PRIMARY KEY,
    user_id BIGINT      NOT NULL,
    role_id BIGINT      NOT NULL
);

CREATE UNIQUE INDEX uk_sys_user_role ON sys_user_role (user_id, role_id);
CREATE INDEX idx_sys_user_role_user_id ON sys_user_role (user_id);
CREATE INDEX idx_sys_user_role_role_id ON sys_user_role (role_id);

COMMENT ON TABLE sys_user_role IS '用户角色关联表';
COMMENT ON COLUMN sys_user_role.id IS '主键ID';
COMMENT ON COLUMN sys_user_role.user_id IS '用户ID（关联 sys_user.id）';
COMMENT ON COLUMN sys_user_role.role_id IS '角色ID（关联 sys_role.id）';

-- ============================================================
-- 测试数据（Super→R_SUPER, Admin→R_ADMIN, User→R_USER）
-- ============================================================

INSERT INTO sys_user_role (user_id, role_id)
VALUES
    (1, 1),
    (2, 2),
    (3, 3);