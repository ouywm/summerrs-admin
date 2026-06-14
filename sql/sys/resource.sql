-- ============================================================
-- 后端 API 资源表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.resource (
    id            BIGSERIAL    PRIMARY KEY,
    resource_name VARCHAR(128) NOT NULL,
    resource_code VARCHAR(128) NOT NULL,
    method        VARCHAR(16)  NOT NULL,
    path          VARCHAR(256) NOT NULL,
    description   VARCHAR(512) NOT NULL DEFAULT '',
    enabled       BOOLEAN      NOT NULL DEFAULT TRUE,
    create_time   TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time   TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_resource_code ON sys.resource (resource_code);
CREATE UNIQUE INDEX uk_sys_resource_method_path ON sys.resource (method, path);
CREATE INDEX idx_sys_resource_enabled ON sys.resource (enabled);

COMMENT ON TABLE sys.resource IS '后端 API 资源表';
COMMENT ON COLUMN sys.resource.id IS '资源ID';
COMMENT ON COLUMN sys.resource.resource_name IS '资源名称';
COMMENT ON COLUMN sys.resource.resource_code IS '资源编码';
COMMENT ON COLUMN sys.resource.method IS 'HTTP 方法';
COMMENT ON COLUMN sys.resource.path IS 'API 路径，支持 {param} 参数形式';
COMMENT ON COLUMN sys.resource.description IS '资源说明';
COMMENT ON COLUMN sys.resource.enabled IS '是否启用资源权限';
COMMENT ON COLUMN sys.resource.create_time IS '创建时间';
COMMENT ON COLUMN sys.resource.update_time IS '更新时间';

-- ============================================================
-- 操作资源关联表
-- 当前 action_menu_id 指向 sys.menu.id，且该菜单应为 menu_type = 2(Button)。
-- ============================================================

CREATE TABLE sys.action_resource (
    id             BIGSERIAL PRIMARY KEY,
    action_menu_id BIGINT    NOT NULL,
    resource_id    BIGINT    NOT NULL
);

CREATE UNIQUE INDEX uk_sys_action_resource ON sys.action_resource (action_menu_id, resource_id);
CREATE INDEX idx_sys_action_resource_action_menu_id ON sys.action_resource (action_menu_id);
CREATE INDEX idx_sys_action_resource_resource_id ON sys.action_resource (resource_id);

COMMENT ON TABLE sys.action_resource IS '操作与后端 API 资源关联表';
COMMENT ON COLUMN sys.action_resource.id IS '主键ID';
COMMENT ON COLUMN sys.action_resource.action_menu_id IS '操作ID，当前复用 sys.menu.id(menu_type=2)';
COMMENT ON COLUMN sys.action_resource.resource_id IS '后端 API 资源ID';
