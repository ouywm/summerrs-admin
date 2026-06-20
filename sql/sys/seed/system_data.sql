-- ============================================================
-- System seed data
-- Data only. Schema is managed by SeaORM entity schema sync.
-- ============================================================

-- Roles
INSERT INTO sys."role" (
    role_name, role_code, description, enabled, create_time, update_time
)
VALUES
    ('超级管理员', 'R_SUPER', '拥有系统所有权限', TRUE, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('管理员',     'R_ADMIN', '拥有大部分管理权限', TRUE, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('普通用户',   'R_USER',  '仅拥有基本操作权限', TRUE, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT (role_code) DO NOTHING;

-- Users
INSERT INTO sys."user" (
    user_name, password, nick_name, gender, phone, email, avatar, status,
    create_by, create_time, update_by, update_time
)
VALUES
    ('Super',  '$2a$10$N.zmdr9k7uOCQb376NoUnuTJ8iAt6Z2Rx1z4TqL9Z0.Dq3GwLFpK6', '超级管理员', 1, '13800138000', 'super@example.com', '', 1, 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
    ('Admin',  '$2a$10$N.zmdr9k7uOCQb376NoUnuTJ8iAt6Z2Rx1z4TqL9Z0.Dq3GwLFpK6', '管理员',     1, '13800138001', 'admin@example.com', '', 1, 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
    ('User',   '$2a$10$N.zmdr9k7uOCQb376NoUnuTJ8iAt6Z2Rx1z4TqL9Z0.Dq3GwLFpK6', '普通用户',   1, '13800138002', 'user@example.com', '', 1, 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP)
ON CONFLICT (user_name) DO NOTHING;

-- User roles
INSERT INTO sys.user_role (user_id, role_id)
SELECT u.id, r.id
FROM (
    VALUES
        ('Super', 'R_SUPER'),
        ('Admin', 'R_ADMIN'),
        ('User', 'R_USER')
) AS mapping(user_name, role_code)
JOIN sys."user" u ON u.user_name = mapping.user_name
JOIN sys."role" r ON r.role_code = mapping.role_code
ON CONFLICT (user_id, role_id) DO NOTHING;

-- Config groups
INSERT INTO sys.config_group (
    group_name, group_code, group_sort, enabled, is_system, remark,
    create_by, create_time, update_by, update_time
) VALUES
    ('基础设置', 'basic', 10, TRUE, TRUE, '站点基础信息相关配置', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
    ('安全设置', 'security', 20, TRUE, TRUE, '登录与账号安全相关配置', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP)
ON CONFLICT (group_code) DO NOTHING;

-- Config values
INSERT INTO sys.config (
    config_name, config_key, config_value, default_value, value_type,
    config_group_id, option_dict_type, config_sort, enabled, is_system,
    remark, create_by, create_time, update_by, update_time
) VALUES
    ('站点名称', 'sys.site.name', 'Summer Admin', 'Summer Admin', 1,
     (SELECT id FROM sys.config_group WHERE group_code = 'basic'), '', 1, TRUE, TRUE,
     '系统显示的站点名称', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
    ('站点 Logo', 'sys.site.logo', '', '', 8,
     (SELECT id FROM sys.config_group WHERE group_code = 'basic'), '', 2, TRUE, TRUE,
     '站点 Logo 图片地址', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
    ('版权信息', 'sys.site.copyright', 'Copyright © 2026 Summer', 'Copyright © 2026 Summer', 1,
     (SELECT id FROM sys.config_group WHERE group_code = 'basic'), '', 3, TRUE, TRUE,
     '页面底部版权文案', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP)
ON CONFLICT (config_key) DO NOTHING;
INSERT INTO sys.config (
    config_name, config_key, config_value, default_value, value_type,
    config_group_id, option_dict_type, config_sort, enabled, is_system,
    remark, create_by, create_time, update_by, update_time
) VALUES
    ('登录验证码开关', 'sys.security.captcha_enabled', 'true', 'true', 3,
     (SELECT id FROM sys.config_group WHERE group_code = 'security'), '', 1, TRUE, TRUE,
     '登录页是否启用验证码', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
    ('用户初始密码', 'sys.user.init_password', '123456', '123456', 7,
     (SELECT id FROM sys.config_group WHERE group_code = 'security'), '', 2, TRUE, TRUE,
     '后台创建用户时使用的初始密码', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
    ('开放注册开关', 'sys.user.register_enabled', 'false', 'false', 3,
     (SELECT id FROM sys.config_group WHERE group_code = 'security'), '', 3, TRUE, TRUE,
     '是否允许新用户自主注册', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP)
ON CONFLICT (config_key) DO NOTHING;

-- Dictionary types
INSERT INTO sys.dict_type (
    dict_name, dict_type, status, is_system, remark,
    create_by, create_time, update_by, update_time
) VALUES
('用户状态', 'user_status', 1, true, '用户账号状态', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
('用户性别', 'user_gender', 1, true, '用户性别选项', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
('系统状态', 'sys_status', 1, true, '通用启用禁用状态', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
('菜单类型', 'menu_type', 1, true, '菜单和按钮类型', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
('业务类型', 'business_type', 1, true, '操作日志业务类型', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP),
('登录状态', 'login_status', 1, true, '登录日志状态', 'system', CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP)
ON CONFLICT (dict_type) DO NOTHING;

-- Dictionary data
INSERT INTO sys.dict_data (
    dict_type, dict_label, dict_value, dict_sort, css_class, list_class,
    is_default, status, is_system, remark, create_by, create_time, update_by, update_time
)
SELECT
    seed.dict_type, seed.dict_label, seed.dict_value, seed.dict_sort,
    '', seed.list_class, FALSE, 1, seed.is_system, '',
    seed.create_by, CURRENT_TIMESTAMP, 'system', CURRENT_TIMESTAMP
FROM (
    VALUES
        ('user_status', '启用', '1', 1, 'success', true, 'system'),
        ('user_status', '禁用', '2', 2, 'danger', true, 'system'),
        ('user_gender', '未知', '0', 1, 'info', true, 'system'),
        ('user_gender', '男', '1', 2, 'primary', true, 'system'),
        ('user_gender', '女', '2', 3, 'danger', true, 'system'),
        ('sys_status', '启用', '1', 1, 'success', true, 'system'),
        ('sys_status', '禁用', '2', 2, 'danger', true, 'system'),
        ('menu_type', '菜单', '1', 1, 'primary', true, 'system'),
        ('menu_type', '按钮', '2', 2, 'success', true, 'system'),
        ('business_type', '其他', '0', 1, 'info', true, 'system'),
        ('business_type', '新增', '1', 2, 'success', true, 'system'),
        ('business_type', '修改', '2', 3, 'primary', true, 'system'),
        ('business_type', '删除', '3', 4, 'danger', true, 'system'),
        ('business_type', '查询', '4', 5, 'info', true, 'system'),
        ('business_type', '导出', '5', 6, 'warning', true, 'system'),
        ('business_type', '导入', '6', 7, 'warning', true, 'system'),
        ('business_type', '认证', '7', 8, 'primary', true, 'system'),
        ('login_status', '成功', '1', 1, 'success', true, 'system'),
        ('login_status', '失败', '2', 2, 'danger', true, 'system')
) AS seed(dict_type, dict_label, dict_value, dict_sort, list_class, is_system, create_by)
WHERE NOT EXISTS (
    SELECT 1 FROM sys.dict_data existing
    WHERE existing.dict_type = seed.dict_type
      AND existing.dict_value = seed.dict_value
);

-- Menus and action buttons
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1, 0, 1, 'Dashboard', '/dashboard', '/index/index', '', 'ri:pie-chart-line', 'menus.dashboard.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (2, 1, 1, 'Console', 'console', '/dashboard/console', '', 'ri:home-smile-2-line', 'menus.dashboard.console', '', false, false, false, false, false, false, true, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (3, 1, 1, 'Analysis', 'analysis', '/dashboard/analysis', '', 'ri:align-item-bottom-line', 'menus.dashboard.analysis', '', false, false, false, false, false, false, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (4, 1, 1, 'Ecommerce', 'ecommerce', '/dashboard/ecommerce', '', 'ri:bar-chart-box-line', 'menus.dashboard.ecommerce', '', false, false, false, false, false, false, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (100, 0, 1, 'System', '/system', '/index/index', '', 'ri:user-3-line', 'menus.system.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (101, 100, 1, 'User', 'user', '/system/user', '', 'ri:user-line', 'menus.system.user', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (102, 100, 1, 'Role', 'role', '/system/role', '', 'ri:user-settings-line', 'menus.system.role', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (103, 100, 1, 'UserCenter', 'user-center', '/system/user-center', '', 'ri:user-line', 'menus.system.userCenter', '', false, true, true, false, false, true, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (104, 100, 1, 'Menus', 'menu', '/system/menu', '', 'ri:menu-line', 'menus.system.menu', '', false, false, false, false, false, true, false, false, '', '', '', '', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (176, 100, 1, 'ResourcePermission', 'resource-permission', '/system/resource-permission', '', 'ri:shield-keyhole-line', 'menus.system.resourcePermission', '', false, false, false, false, false, true, false, false, '', '', '', '', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (178, 100, 1, 'SystemFeedback', 'feedback', '/system/feedback', '', 'ri:chat-3-line', 'menus.system.feedback', '', false, false, false, false, false, true, false, false, '', '', '', '', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (126, 100, 1, 'Config', 'config', '/system/config', '', 'ri:settings-3-line', 'menus.system.config', '', false, false, false, false, false, true, false, false, '', '', '', '', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (127, 100, 1, 'Notice', 'notice', '/system/notice', '', 'ri:notification-3-line', 'menus.system.notice', '', false, false, false, false, false, true, false, false, '', '', '', '', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (128, 100, 1, 'UserNotice', 'notice-center', '/system/user-notice', '', '', 'menus.system.noticeCenter', '', false, true, false, false, false, true, false, false, '', '', '', '', 9, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (129, 100, 1, 'UserNoticeDetail', 'notice-center/:id', '/system/user-notice/detail', '', '', 'menus.system.noticeDetail', '', false, true, false, false, false, false, false, false, '', '/system/notice-center', '', '', 10, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (130, 100, 1, 'Online', 'online', '/system/online', '', 'ri:user-shared-line', 'menus.system.online', '', false, false, false, false, false, true, false, false, '', '', '', '', 11, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (131, 100, 1, 'Dict', 'dict', '/system/dict', '', 'ri:book-2-line', 'menus.system.dict', '', false, false, false, false, false, true, false, false, '', '', '', '', 12, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (132, 100, 1, 'LoginLog', 'login-log', '/system/login-log', '', 'ri:file-list-3-line', 'menus.system.loginLog', '', false, false, false, false, false, true, false, false, '', '', '', '', 13, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (133, 100, 1, 'OperationLog', 'operation-log', '/system/operation-log', '', 'ri:file-text-line', 'menus.system.operationLog', '', false, false, false, false, false, true, false, false, '', '', '', '', 14, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (105, 104, 2, '', '', '', '', '', '新增', '', false, false, false, false, false, false, false, false, '', '', '新增', 'add', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (106, 104, 2, '', '', '', '', '', '编辑', '', false, false, false, false, false, false, false, false, '', '', '编辑', 'edit', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (107, 104, 2, '', '', '', '', '', '删除', '', false, false, false, false, false, false, false, false, '', '', '删除', 'delete', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (116, 101, 2, '', '', '', '', '', '查询用户', '', false, false, false, false, false, false, false, false, '', '', '查询用户', 'system:user:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (117, 101, 2, '', '', '', '', '', '新增用户', '', false, false, false, false, false, false, false, false, '', '', '新增用户', 'system:user:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (118, 101, 2, '', '', '', '', '', '编辑用户', '', false, false, false, false, false, false, false, false, '', '', '编辑用户', 'system:user:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (119, 101, 2, '', '', '', '', '', '删除用户', '', false, false, false, false, false, false, false, false, '', '', '删除用户', 'system:user:delete', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (120, 101, 2, '', '', '', '', '', '重置用户密码', '', false, false, false, false, false, false, false, false, '', '', '重置用户密码', 'system:user:reset-password', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1200, 101, 2, '', '', '', '', '', '查看用户详情', '', false, false, false, false, false, false, false, false, '', '', '查看用户详情', 'system:user:detail', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (121, 102, 2, '', '', '', '', '', '查询角色', '', false, false, false, false, false, false, false, false, '', '', '查询角色', 'system:role:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (122, 102, 2, '', '', '', '', '', '新增角色', '', false, false, false, false, false, false, false, false, '', '', '新增角色', 'system:role:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (123, 102, 2, '', '', '', '', '', '编辑角色', '', false, false, false, false, false, false, false, false, '', '', '编辑角色', 'system:role:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (124, 102, 2, '', '', '', '', '', '删除角色', '', false, false, false, false, false, false, false, false, '', '', '删除角色', 'system:role:delete', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (125, 102, 2, '', '', '', '', '', '角色权限', '', false, false, false, false, false, false, false, false, '', '', '角色权限', 'system:role:permission', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (134, 104, 2, '', '', '', '', '', '查询菜单', '', false, false, false, false, false, false, false, false, '', '', '查询菜单', 'system:menu:list', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (135, 104, 2, '', '', '', '', '', '新增菜单', '', false, false, false, false, false, false, false, false, '', '', '新增菜单', 'system:menu:create', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (136, 104, 2, '', '', '', '', '', '编辑菜单', '', false, false, false, false, false, false, false, false, '', '', '编辑菜单', 'system:menu:update', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (137, 104, 2, '', '', '', '', '', '删除菜单', '', false, false, false, false, false, false, false, false, '', '', '删除菜单', 'system:menu:delete', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (138, 104, 2, '', '', '', '', '', '新增按钮', '', false, false, false, false, false, false, false, false, '', '', '新增按钮', 'system:button:create', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (139, 104, 2, '', '', '', '', '', '编辑按钮', '', false, false, false, false, false, false, false, false, '', '', '编辑按钮', 'system:button:update', 9, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (177, 104, 2, '', '', '', '', '', '绑定资源', '', false, false, false, false, false, false, false, false, '', '', '绑定资源', 'bind', 10, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (170, 176, 2, '', '', '', '', '', '查询资源', '', false, false, false, false, false, false, false, false, '', '', '查询资源', 'system:resource:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (171, 176, 2, '', '', '', '', '', '新增资源', '', false, false, false, false, false, false, false, false, '', '', '新增资源', 'system:resource:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (172, 176, 2, '', '', '', '', '', '编辑资源', '', false, false, false, false, false, false, false, false, '', '', '编辑资源', 'system:resource:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (173, 176, 2, '', '', '', '', '', '删除资源', '', false, false, false, false, false, false, false, false, '', '', '删除资源', 'system:resource:delete', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (174, 176, 2, '', '', '', '', '', '绑定资源', '', false, false, false, false, false, false, false, false, '', '', '绑定资源', 'system:resource:bind', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (175, 176, 2, '', '', '', '', '', '刷新资源权限', '', false, false, false, false, false, false, false, false, '', '', '刷新资源权限', 'system:resource:reload', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1205, 176, 2, '', '', '', '', '', '查看资源详情', '', false, false, false, false, false, false, false, false, '', '', '查看资源详情', 'system:resource:detail', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (140, 126, 2, '', '', '', '', '', '查询配置', '', false, false, false, false, false, false, false, false, '', '', '查询配置', 'system:config:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (141, 126, 2, '', '', '', '', '', '新增配置', '', false, false, false, false, false, false, false, false, '', '', '新增配置', 'system:config:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (142, 126, 2, '', '', '', '', '', '编辑配置', '', false, false, false, false, false, false, false, false, '', '', '编辑配置', 'system:config:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (143, 126, 2, '', '', '', '', '', '删除配置', '', false, false, false, false, false, false, false, false, '', '', '删除配置', 'system:config:delete', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (144, 126, 2, '', '', '', '', '', '查询配置分组', '', false, false, false, false, false, false, false, false, '', '', '查询配置分组', 'system:config-group:list', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (145, 126, 2, '', '', '', '', '', '新增配置分组', '', false, false, false, false, false, false, false, false, '', '', '新增配置分组', 'system:config-group:create', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (146, 126, 2, '', '', '', '', '', '编辑配置分组', '', false, false, false, false, false, false, false, false, '', '', '编辑配置分组', 'system:config-group:update', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (147, 126, 2, '', '', '', '', '', '删除配置分组', '', false, false, false, false, false, false, false, false, '', '', '删除配置分组', 'system:config-group:delete', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1201, 126, 2, '', '', '', '', '', '查看配置详情', '', false, false, false, false, false, false, false, false, '', '', '查看配置详情', 'system:config:detail', 9, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1202, 126, 2, '', '', '', '', '', '查看配置分组详情', '', false, false, false, false, false, false, false, false, '', '', '查看配置分组详情', 'system:config-group:detail', 10, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (148, 127, 2, '', '', '', '', '', '查询公告', '', false, false, false, false, false, false, false, false, '', '', '查询公告', 'system:notice:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (149, 127, 2, '', '', '', '', '', '新增公告', '', false, false, false, false, false, false, false, false, '', '', '新增公告', 'system:notice:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (150, 127, 2, '', '', '', '', '', '编辑公告', '', false, false, false, false, false, false, false, false, '', '', '编辑公告', 'system:notice:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (151, 127, 2, '', '', '', '', '', '删除公告', '', false, false, false, false, false, false, false, false, '', '', '删除公告', 'system:notice:delete', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (152, 127, 2, '', '', '', '', '', '发布公告', '', false, false, false, false, false, false, false, false, '', '', '发布公告', 'system:notice:publish', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (153, 127, 2, '', '', '', '', '', '撤回公告', '', false, false, false, false, false, false, false, false, '', '', '撤回公告', 'system:notice:revoke', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (154, 127, 2, '', '', '', '', '', '置顶公告', '', false, false, false, false, false, false, false, false, '', '', '置顶公告', 'system:notice:pin', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1203, 127, 2, '', '', '', '', '', '查看公告详情', '', false, false, false, false, false, false, false, false, '', '', '查看公告详情', 'system:notice:detail', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (155, 128, 2, '', '', '', '', '', '查询通知中心', '', false, false, false, false, false, false, false, false, '', '', '查询通知中心', 'system:user-notice:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (156, 128, 2, '', '', '', '', '', '读取通知', '', false, false, false, false, false, false, false, false, '', '', '读取通知', 'system:user-notice:read', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1204, 128, 2, '', '', '', '', '', '查看通知详情', '', false, false, false, false, false, false, false, false, '', '', '查看通知详情', 'system:user-notice:detail', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (157, 130, 2, '', '', '', '', '', '查询在线用户', '', false, false, false, false, false, false, false, false, '', '', '查询在线用户', 'system:online:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (158, 130, 2, '', '', '', '', '', '强制下线', '', false, false, false, false, false, false, false, false, '', '', '强制下线', 'system:online:kick', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (159, 131, 2, '', '', '', '', '', '查询字典类型', '', false, false, false, false, false, false, false, false, '', '', '查询字典类型', 'system:dict-type:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (160, 131, 2, '', '', '', '', '', '新增字典类型', '', false, false, false, false, false, false, false, false, '', '', '新增字典类型', 'system:dict-type:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (161, 131, 2, '', '', '', '', '', '编辑字典类型', '', false, false, false, false, false, false, false, false, '', '', '编辑字典类型', 'system:dict-type:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (162, 131, 2, '', '', '', '', '', '删除字典类型', '', false, false, false, false, false, false, false, false, '', '', '删除字典类型', 'system:dict-type:delete', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (163, 131, 2, '', '', '', '', '', '查询字典数据', '', false, false, false, false, false, false, false, false, '', '', '查询字典数据', 'system:dict-data:list', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (164, 131, 2, '', '', '', '', '', '新增字典数据', '', false, false, false, false, false, false, false, false, '', '', '新增字典数据', 'system:dict-data:create', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (165, 131, 2, '', '', '', '', '', '编辑字典数据', '', false, false, false, false, false, false, false, false, '', '', '编辑字典数据', 'system:dict-data:update', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (166, 131, 2, '', '', '', '', '', '删除字典数据', '', false, false, false, false, false, false, false, false, '', '', '删除字典数据', 'system:dict-data:delete', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (167, 132, 2, '', '', '', '', '', '查询登录日志', '', false, false, false, false, false, false, false, false, '', '', '查询登录日志', 'system:login-log:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (168, 133, 2, '', '', '', '', '', '查询操作日志', '', false, false, false, false, false, false, false, false, '', '', '查询操作日志', 'system:operation-log:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (169, 133, 2, '', '', '', '', '', '查看操作日志', '', false, false, false, false, false, false, false, false, '', '', '查看操作日志', 'system:operation-log:detail', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (108, 100, 1, 'Nested', 'nested', '', '', 'ri:menu-unfold-3-line', 'menus.system.nested', '', false, false, false, false, false, true, false, false, '', '', '', '', 13, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (109, 108, 1, 'NestedMenu1', 'menu1', '/system/nested/menu1', '', 'ri:align-justify', 'menus.system.menu1', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (110, 108, 1, 'NestedMenu2', 'menu2', '', '', 'ri:align-justify', 'menus.system.menu2', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (111, 110, 1, 'NestedMenu2-1', 'menu2-1', '/system/nested/menu2', '', 'ri:align-justify', 'menus.system.menu21', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (112, 108, 1, 'NestedMenu3', 'menu3', '', '', 'ri:align-justify', 'menus.system.menu3', '', false, false, false, false, false, true, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (113, 112, 1, 'NestedMenu3-1', 'menu3-1', '/system/nested/menu3', '', '', 'menus.system.menu31', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (114, 112, 1, 'NestedMenu3-2', 'menu3-2', '', '', '', 'menus.system.menu32', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (115, 114, 1, 'NestedMenu3-2-1', 'menu3-2-1', '/system/nested/menu3/menu3-2', '', '', 'menus.system.menu321', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (200, 0, 1, 'Article', '/article', '/index/index', '', 'ri:book-2-line', 'menus.article.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (201, 200, 1, 'ArticleList', 'article-list', '/article/list', '', 'ri:article-line', 'menus.article.articleList', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (202, 201, 2, '', '', '', '', '', '新增', '', false, false, false, false, false, false, false, false, '', '', '新增', 'add', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (203, 201, 2, '', '', '', '', '', '编辑', '', false, false, false, false, false, false, false, false, '', '', '编辑', 'edit', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (204, 200, 1, 'ArticleDetail', 'detail/:id', '/article/detail', '', '', 'menus.article.articleDetail', '', false, true, false, false, false, true, false, false, '', '/article/article-list', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (205, 200, 1, 'ArticleComment', 'comment', '/article/comment', '', 'ri:mail-line', 'menus.article.comment', '', false, false, false, false, false, true, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (206, 200, 1, 'ArticlePublish', 'publish', '/article/publish', '', 'ri:telegram-2-line', 'menus.article.articlePublish', '', false, false, false, false, false, true, false, false, '', '', '', '', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (207, 206, 2, '', '', '', '', '', '发布', '', false, false, false, false, false, false, false, false, '', '', '发布', 'add', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (300, 0, 1, 'Template', '/template', '/index/index', '', 'ri:apps-2-line', 'menus.template.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (301, 300, 1, 'Cards', 'cards', '/template/cards', '', 'ri:wallet-line', 'menus.template.cards', '', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (302, 300, 1, 'Banners', 'banners', '/template/banners', '', 'ri:rectangle-line', 'menus.template.banners', '', false, false, false, false, false, false, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (303, 300, 1, 'Charts', 'charts', '/template/charts', '', 'ri:bar-chart-box-line', 'menus.template.charts', '', false, false, false, false, false, false, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (304, 300, 1, 'Map', 'map', '/template/map', '', 'ri:map-pin-line', 'menus.template.map', '', false, false, false, false, false, true, false, false, '', '', '', '', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (305, 300, 1, 'Chat', 'chat', '/template/chat', '', 'ri:message-3-line', 'menus.template.chat', '', false, false, false, false, false, true, false, false, '', '', '', '', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (306, 300, 1, 'Calendar', 'calendar', '/template/calendar', '', 'ri:calendar-2-line', 'menus.template.calendar', '', false, false, false, false, false, true, false, false, '', '', '', '', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (307, 300, 1, 'Pricing', 'pricing', '/template/pricing', '', 'ri:money-cny-box-line', 'menus.template.pricing', '', false, false, false, true, false, true, false, false, '', '', '', '', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (400, 0, 1, 'Widgets', '/widgets', '/index/index', '', 'ri:apps-2-add-line', 'menus.widgets.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (401, 400, 1, 'Icon', 'icon', '/widgets/icon', '', 'ri:palette-line', 'menus.widgets.icon', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (402, 400, 1, 'ImageCrop', 'image-crop', '/widgets/image-crop', '', 'ri:screenshot-line', 'menus.widgets.imageCrop', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (403, 400, 1, 'Excel', 'excel', '/widgets/excel', '', 'ri:download-2-line', 'menus.widgets.excel', '', false, false, false, false, false, true, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (404, 400, 1, 'Video', 'video', '/widgets/video', '', 'ri:vidicon-line', 'menus.widgets.video', '', false, false, false, false, false, true, false, false, '', '', '', '', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (405, 400, 1, 'CountTo', 'count-to', '/widgets/count-to', '', 'ri:anthropic-line', 'menus.widgets.countTo', '', false, false, false, false, false, false, false, false, '', '', '', '', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (406, 400, 1, 'WangEditor', 'wang-editor', '/widgets/wang-editor', '', 'ri:t-box-line', 'menus.widgets.wangEditor', '', false, false, false, false, false, true, false, false, '', '', '', '', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (407, 400, 1, 'Watermark', 'watermark', '/widgets/watermark', '', 'ri:water-flash-line', 'menus.widgets.watermark', '', false, false, false, false, false, true, false, false, '', '', '', '', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (408, 400, 1, 'ContextMenu', 'context-menu', '/widgets/context-menu', '', 'ri:menu-2-line', 'menus.widgets.contextMenu', '', false, false, false, false, false, true, false, false, '', '', '', '', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (409, 400, 1, 'Qrcode', 'qrcode', '/widgets/qrcode', '', 'ri:qr-code-line', 'menus.widgets.qrcode', '', false, false, false, false, false, true, false, false, '', '', '', '', 9, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (410, 400, 1, 'Drag', 'drag', '/widgets/drag', '', 'ri:drag-move-fill', 'menus.widgets.drag', '', false, false, false, false, false, true, false, false, '', '', '', '', 10, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (411, 400, 1, 'WidgetsDataSelect', 'data-select', '/widgets/data-select', '', 'ri:list-check-3', 'menus.widgets.dataSelect', '', false, false, false, false, false, true, false, false, '', '', '', '', 11, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (412, 400, 1, 'WidgetsResourceSelect', 'resource-select', '/widgets/resource-select', '', 'ri:folder-image-line', 'menus.widgets.resourceSelect', '', false, false, false, false, false, true, false, false, '', '', '', '', 12, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (413, 400, 1, 'TextScroll', 'text-scroll', '/widgets/text-scroll', '', 'ri:input-method-line', 'menus.widgets.textScroll', '', false, false, false, false, false, true, false, false, '', '', '', '', 13, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (414, 400, 1, 'Fireworks', 'fireworks', '/widgets/fireworks', '', 'ri:magic-line', 'menus.widgets.fireworks', '', false, false, false, false, false, true, false, false, 'Hot', '', '', '', 14, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (415, 400, 1, 'ElementUI', '/outside/iframe/elementui', '', '', 'ri:apps-2-line', 'menus.widgets.elementUI', 'https://element-plus.org/zh-CN/component/overview.html', true, false, false, false, false, false, false, false, '', '', '', '', 15, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (500, 0, 1, 'Examples', '/examples', '/index/index', '', 'ri:sparkling-line', 'menus.examples.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (501, 500, 1, 'Permission', 'permission', '', '', 'ri:fingerprint-line', 'menus.examples.permission.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (502, 501, 1, 'PermissionSwitchRole', 'switch-role', '/examples/permission/switch-role', '', 'ri:contacts-line', 'menus.examples.permission.switchRole', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (503, 501, 1, 'PermissionButtonAuth', 'button-auth', '/examples/permission/button-auth', '', 'ri:mouse-line', 'menus.examples.permission.buttonAuth', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (504, 503, 2, '', '', '', '', '', '新增', '', false, false, false, false, false, false, false, false, '', '', '新增', 'add', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (505, 503, 2, '', '', '', '', '', '编辑', '', false, false, false, false, false, false, false, false, '', '', '编辑', 'edit', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (506, 503, 2, '', '', '', '', '', '删除', '', false, false, false, false, false, false, false, false, '', '', '删除', 'delete', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (507, 503, 2, '', '', '', '', '', '导出', '', false, false, false, false, false, false, false, false, '', '', '导出', 'export', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (508, 503, 2, '', '', '', '', '', '查看', '', false, false, false, false, false, false, false, false, '', '', '查看', 'view', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (509, 503, 2, '', '', '', '', '', '发布', '', false, false, false, false, false, false, false, false, '', '', '发布', 'publish', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (510, 503, 2, '', '', '', '', '', '配置', '', false, false, false, false, false, false, false, false, '', '', '配置', 'config', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (511, 503, 2, '', '', '', '', '', '管理', '', false, false, false, false, false, false, false, false, '', '', '管理', 'manage', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (512, 501, 1, 'PermissionPageVisibility', 'page-visibility', '/examples/permission/page-visibility', '', 'ri:user-3-line', 'menus.examples.permission.pageVisibility', '', false, false, false, false, false, true, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (513, 500, 1, 'Tabs', 'tabs', '/examples/tabs', '', 'ri:price-tag-line', 'menus.examples.tabs', '', false, false, false, false, false, false, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (514, 500, 1, 'TablesBasic', 'tables/basic', '/examples/tables/basic', '', 'ri:layout-grid-line', 'menus.examples.tablesBasic', '', false, false, false, false, false, true, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (515, 500, 1, 'Tables', 'tables', '/examples/tables', '', 'ri:table-3', 'menus.examples.tables', '', false, false, false, false, false, true, false, false, '', '', '', '', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (516, 500, 1, 'Forms', 'forms', '/examples/forms', '', 'ri:table-view', 'menus.examples.forms', '', false, false, false, false, false, true, false, false, '', '', '', '', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (517, 500, 1, 'SearchBar', 'form/search-bar', '/examples/forms/search-bar', '', 'ri:table-line', 'menus.examples.searchBar', '', false, false, false, false, false, true, false, false, '', '', '', '', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (518, 500, 1, 'TablesTree', 'tables/tree', '/examples/tables/tree', '', 'ri:layout-2-line', 'menus.examples.tablesTree', '', false, false, false, false, false, true, false, false, '', '', '', '', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (519, 500, 1, 'SocketChat', 'socket-chat', '/examples/socket-chat', '', 'ri:shake-hands-line', 'menus.examples.socketChat', '', false, false, false, false, false, true, false, false, 'New', '', '', '', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (600, 0, 1, 'Result', '/result', '/index/index', '', 'ri:checkbox-circle-line', 'menus.result.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (601, 600, 1, 'ResultSuccess', 'success', '/result/success', '', 'ri:checkbox-circle-line', 'menus.result.success', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (602, 600, 1, 'ResultFail', 'fail', '/result/fail', '', 'ri:close-circle-line', 'menus.result.fail', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (700, 0, 1, 'Exception', '/exception', '/index/index', '', 'ri:error-warning-line', 'menus.exception.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (701, 700, 1, 'Exception403', '403', '/exception/403', '', '', 'menus.exception.forbidden', '', false, false, true, true, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (702, 700, 1, 'Exception404', '404', '/exception/404', '', '', 'menus.exception.notFound', '', false, false, true, true, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (703, 700, 1, 'Exception500', '500', '/exception/500', '', '', 'menus.exception.serverError', '', false, false, true, true, false, true, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (800, 0, 1, 'Safeguard', '/safeguard', '/index/index', '', 'ri:shield-check-line', 'menus.safeguard.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 9, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (801, 800, 1, 'SafeguardServer', 'server', '/safeguard/server', '', 'ri:hard-drive-3-line', 'menus.safeguard.server', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (805, 0, 1, 'Monitor', '/monitor', '/index/index', '', 'ri:pulse-line', 'menus.monitor.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 10, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (806, 805, 1, 'ServerMonitor', 'server', '/monitor/server', '', 'ri:server-line', 'menus.monitor.server', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (807, 805, 1, 'CacheMonitor', 'cache', '/monitor/cache', '', 'ri:database-2-line', 'menus.monitor.cache', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (802, 806, 2, '', '', '', '', '', '查询服务器监控', '', false, false, false, false, false, false, false, false, '', '', '查询服务器监控', 'system:monitor:server', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (803, 807, 2, '', '', '', '', '', '查询缓存监控', '', false, false, false, false, false, false, false, false, '', '', '查询缓存监控', 'system:monitor:cache', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (804, 807, 2, '', '', '', '', '', '清理缓存', '', false, false, false, false, false, false, false, false, '', '', '清理缓存', 'system:monitor:cache-delete', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1000, 0, 1, 'Scheduler', '/scheduler', '/index/index', '', 'ri:timer-line', 'menus.scheduler.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 10, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1001, 1000, 1, 'SchedulerJob', 'job', '/scheduler/job', '', 'ri:list-check-2', 'menus.scheduler.job', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1002, 1000, 1, 'SchedulerRun', 'run', '/scheduler/run', '', 'ri:history-line', 'menus.scheduler.run', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1003, 1001, 2, '', '', '', '', '', '查询任务', '', false, false, false, false, false, false, false, false, '', '', '查询任务', 'scheduler:job:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1004, 1001, 2, '', '', '', '', '', '新增任务', '', false, false, false, false, false, false, false, false, '', '', '新增任务', 'scheduler:job:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1005, 1001, 2, '', '', '', '', '', '编辑任务', '', false, false, false, false, false, false, false, false, '', '', '编辑任务', 'scheduler:job:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1006, 1001, 2, '', '', '', '', '', '启停任务', '', false, false, false, false, false, false, false, false, '', '', '启停任务', 'scheduler:job:toggle', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1007, 1001, 2, '', '', '', '', '', '触发任务', '', false, false, false, false, false, false, false, false, '', '', '触发任务', 'scheduler:job:trigger', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1008, 1001, 2, '', '', '', '', '', '删除任务', '', false, false, false, false, false, false, false, false, '', '', '删除任务', 'scheduler:job:delete', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1206, 1001, 2, '', '', '', '', '', '查看任务详情', '', false, false, false, false, false, false, false, false, '', '', '查看任务详情', 'scheduler:job:detail', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1009, 1002, 2, '', '', '', '', '', '查询执行记录', '', false, false, false, false, false, false, false, false, '', '', '查询执行记录', 'scheduler:run:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1010, 1002, 2, '', '', '', '', '', '查看执行记录', '', false, false, false, false, false, false, false, false, '', '', '查看执行记录', 'scheduler:run:detail', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1100, 0, 1, 'File', '/file', '/index/index', '', 'ri:folder-3-line', 'menus.file.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 11, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1101, 1100, 1, 'FileManage', 'manage', '/file/manage', '', 'ri:file-list-3-line', 'menus.file.manage', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1102, 1100, 1, 'FileExamples', 'examples', '/file/examples', '', 'ri:upload-cloud-2-line', 'menus.file.examples', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1103, 1101, 2, '', '', '', '', '', '查询文件', '', false, false, false, false, false, false, false, false, '', '', '查询文件', 'file:manage:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1104, 1101, 2, '', '', '', '', '', '上传文件', '', false, false, false, false, false, false, false, false, '', '', '上传文件', 'file:manage:upload', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1105, 1101, 2, '', '', '', '', '', '编辑文件', '', false, false, false, false, false, false, false, false, '', '', '编辑文件', 'file:manage:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1106, 1101, 2, '', '', '', '', '', '删除文件', '', false, false, false, false, false, false, false, false, '', '', '删除文件', 'file:manage:delete', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1107, 1101, 2, '', '', '', '', '', '分享文件', '', false, false, false, false, false, false, false, false, '', '', '分享文件', 'file:manage:share', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1108, 1101, 2, '', '', '', '', '', '新增目录', '', false, false, false, false, false, false, false, false, '', '', '新增目录', 'file:folder:create', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1109, 1101, 2, '', '', '', '', '', '编辑目录', '', false, false, false, false, false, false, false, false, '', '', '编辑目录', 'file:folder:update', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1110, 1101, 2, '', '', '', '', '', '删除目录', '', false, false, false, false, false, false, false, false, '', '', '删除目录', 'file:folder:delete', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1111, 1101, 2, '', '', '', '', '', '下载文件', '', false, false, false, false, false, false, false, false, '', '', '下载文件', 'file:manage:download', 9, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1207, 1101, 2, '', '', '', '', '', '查看文件详情', '', false, false, false, false, false, false, false, false, '', '', '查看文件详情', 'file:manage:detail', 10, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1208, 1101, 2, '', '', '', '', '', '查看目录详情', '', false, false, false, false, false, false, false, false, '', '', '查看目录详情', 'file:folder:detail', 11, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1209, 1101, 2, '', '', '', '', '', '查询目录树', '', false, false, false, false, false, false, false, false, '', '', '查询目录树', 'file:folder:tree', 12, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (900, 0, 1, 'Document', '', '', '', 'ri:bill-line', 'menus.help.document', 'https://docs.example.com', false, false, false, false, false, false, false, false, '', '', '', '', 13, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (901, 0, 1, 'LiteVersion', '', '', '', 'ri:bus-2-line', 'menus.help.liteVersion', 'https://lite.example.com', false, false, false, false, false, false, false, false, '', '', '', '', 14, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (902, 0, 1, 'OldVersion', '', '', '', 'ri:subway-line', 'menus.help.oldVersion', 'https://old.example.com', false, false, false, false, false, false, false, false, '', '', '', '', 15, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (903, 0, 1, 'ChangeLog', '/change/log', '/change/log', '', 'ri:gamepad-line', 'menus.plan.log', '', false, false, false, false, false, false, false, false, 'v3.0.1', '', '', '', 16, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) ON CONFLICT (id) DO NOTHING;

-- Backfill button bit positions
WITH missing_bit_positions AS (
    SELECT
        id,
        (
            COALESCE(
                (SELECT MAX(bit_position) + 1 FROM sys.menu WHERE menu_type = 2 AND bit_position IS NOT NULL),
                0
            ) + ROW_NUMBER() OVER (ORDER BY id) - 1
        )::INTEGER AS next_bit_position
    FROM sys.menu
    WHERE menu_type = 2 AND bit_position IS NULL
)
UPDATE sys.menu AS m
SET bit_position = missing_bit_positions.next_bit_position
FROM missing_bit_positions
WHERE m.id = missing_bit_positions.id;

SELECT setval('sys.menu_id_seq', COALESCE((SELECT MAX(id) FROM sys.menu), 1));

-- Role menus
-- R_SUPER → 全部菜单
INSERT INTO sys.role_menu (role_id, menu_id)
SELECT r.id, m.id
FROM sys."role" r
CROSS JOIN sys.menu m
WHERE r.role_code = 'R_SUPER'
ON CONFLICT (role_id, menu_id) DO NOTHING;

-- R_ADMIN → 全部菜单，排除"菜单管理"(Menus)及其下挂按钮
INSERT INTO sys.role_menu (role_id, menu_id)
SELECT r.id, m.id
FROM sys."role" r
CROSS JOIN sys.menu m
WHERE r.role_code = 'R_ADMIN'
  AND m.id NOT IN (SELECT id FROM sys.menu WHERE name = 'Menus')
  AND m.parent_id NOT IN (SELECT id FROM sys.menu WHERE name = 'Menus')
ON CONFLICT (role_id, menu_id) DO NOTHING;

-- R_USER → 仅仪表盘（根节点 Dashboard 及其直接子页）
INSERT INTO sys.role_menu (role_id, menu_id)
SELECT r.id, m.id
FROM sys."role" r
CROSS JOIN sys.menu m
WHERE r.role_code = 'R_USER'
  AND (m.name = 'Dashboard'
       OR m.parent_id IN (SELECT id FROM sys.menu WHERE name = 'Dashboard'))
ON CONFLICT (role_id, menu_id) DO NOTHING;

-- Resources and action-resource bindings
INSERT INTO sys.resource (
    resource_name, resource_code, method, path, description,
    enabled, create_time, update_time
)
SELECT
    seed.resource_name, seed.resource_code, seed.method, seed.path, seed.description,
    TRUE, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
FROM (
    VALUES
    ('当前用户菜单树', 'system:menu:current-tree.query', 'GET', '/api/v3/system/menus', '查询当前登录用户可见菜单树'),
    ('当前用户信息', 'system:user:info.query', 'GET', '/api/user/info', '查询当前登录用户信息'),
    ('用户列表', 'system:user:list.query', 'GET', '/api/user/list', '用户管理列表查询'),
    ('用户详情', 'system:user:detail.query', 'GET', '/api/user/{id}', '用户详情查询'),
    ('创建用户', 'system:user:create.submit', 'POST', '/api/user', '提交创建用户'),
    ('更新用户', 'system:user:update.submit', 'PUT', '/api/user/{id}', '提交更新用户'),
    ('删除用户', 'system:user:delete.submit', 'DELETE', '/api/user/{id}', '删除用户'),
    ('重置用户密码', 'system:user:reset-password.submit', 'PUT', '/api/user/{id}/reset-password', '重置用户密码'),
    ('角色列表', 'system:role:list.query', 'GET', '/api/role/list', '角色管理列表查询'),
    ('创建角色', 'system:role:create.submit', 'POST', '/api/role', '提交创建角色'),
    ('更新角色', 'system:role:update.submit', 'PUT', '/api/role/{role_id}', '提交更新角色'),
    ('删除角色', 'system:role:delete.submit', 'DELETE', '/api/role/{role_id}', '删除角色'),
    ('角色权限详情', 'system:role:permission.query', 'GET', '/api/role/{role_id}/permissions', '查询角色菜单权限'),
    ('保存角色权限', 'system:role:permission.submit', 'PUT', '/api/role/{role_id}/permissions', '保存角色菜单权限'),
    ('菜单列表', 'system:menu:list.query', 'GET', '/api/system/menu/list', '菜单管理列表查询'),
    ('创建菜单', 'system:menu:create.submit', 'POST', '/api/system/menu', '提交创建菜单'),
    ('创建按钮', 'system:button:create.submit', 'POST', '/api/system/button', '提交创建按钮'),
    ('更新菜单', 'system:menu:update.submit', 'PUT', '/api/system/menu/{id}', '提交更新菜单'),
    ('更新按钮', 'system:button:update.submit', 'PUT', '/api/system/button/{id}', '提交更新按钮'),
    ('删除菜单或按钮', 'system:menu:delete.submit', 'DELETE', '/api/system/menu/{id}', '删除菜单或按钮'),
    ('配置分组列表', 'system:config-group:list.query', 'GET', '/api/config/group/list', '配置分组列表查询'),
    ('配置分组详情', 'system:config-group:detail.query', 'GET', '/api/config/group/{id}', '配置分组详情查询'),
    ('创建配置分组', 'system:config-group:create.submit', 'POST', '/api/config/group', '提交创建配置分组'),
    ('更新配置分组', 'system:config-group:update.submit', 'PUT', '/api/config/group/{id}', '提交更新配置分组'),
    ('删除配置分组', 'system:config-group:delete.submit', 'DELETE', '/api/config/group/{id}', '删除配置分组'),
    ('分组配置列表', 'system:config:list.query', 'GET', '/api/config/grouped', '按分组查询系统配置'),
    ('配置 Key 查询', 'system:config:by-key.query', 'GET', '/api/config/by-key/{config_key}', '按配置 Key 查询配置值'),
    ('配置 Key 批量查询', 'system:config:by-keys.query', 'POST', '/api/config/by-keys', '按配置 Key 批量查询配置值'),
    ('配置详情', 'system:config:detail.query', 'GET', '/api/config/{id}', '系统配置详情查询'),
    ('创建配置', 'system:config:create.submit', 'POST', '/api/config', '提交创建系统配置'),
    ('更新配置', 'system:config:update.submit', 'PUT', '/api/config/{id}', '提交更新系统配置'),
    ('删除配置', 'system:config:delete.submit', 'DELETE', '/api/config/{id}', '删除系统配置'),
    ('公告列表', 'system:notice:list.query', 'GET', '/api/notice/list', '公告列表查询'),
    ('公告详情', 'system:notice:detail.query', 'GET', '/api/notice/{id}', '公告详情查询'),
    ('创建公告', 'system:notice:create.submit', 'POST', '/api/notice', '提交创建公告'),
    ('更新公告', 'system:notice:update.submit', 'PUT', '/api/notice/{id}', '提交更新公告'),
    ('删除公告', 'system:notice:delete.submit', 'DELETE', '/api/notice/{id}', '删除公告'),
    ('发布公告', 'system:notice:publish.submit', 'PUT', '/api/notice/{id}/publish', '发布公告'),
    ('撤回公告', 'system:notice:revoke.submit', 'PUT', '/api/notice/{id}/revoke', '撤回公告'),
    ('置顶公告', 'system:notice:pin.submit', 'PUT', '/api/notice/{id}/pin', '置顶公告'),
    ('取消置顶公告', 'system:notice:unpin.submit', 'PUT', '/api/notice/{id}/unpin', '取消置顶公告'),
    ('通知中心列表', 'system:user-notice:list.query', 'GET', '/api/user/notice/list', '当前用户通知列表查询'),
    ('最新通知', 'system:user-notice:latest.query', 'GET', '/api/user/notice/latest', '当前用户最新通知查询'),
    ('未读通知数', 'system:user-notice:unread-count.query', 'GET', '/api/user/notice/unread-count', '当前用户未读通知数量查询'),
    ('通知详情', 'system:user-notice:detail.query', 'GET', '/api/user/notice/{id}', '当前用户通知详情查询'),
    ('标记通知已读', 'system:user-notice:read.submit', 'PUT', '/api/user/notice/{id}/read', '标记单条通知已读'),
    ('全部通知已读', 'system:user-notice:read-all.submit', 'PUT', '/api/user/notice/read-all', '标记全部通知已读'),
    ('在线用户列表', 'system:online:list.query', 'GET', '/api/online/list', '在线用户列表查询'),
    ('强制下线用户', 'system:online:kick.submit', 'DELETE', '/api/online/{login_id}', '强制指定登录用户下线'),
    ('强制下线用户设备', 'system:online:kick-device.submit', 'DELETE', '/api/online/{login_id}/{device}', '强制指定登录用户设备下线'),
    ('字典类型列表', 'system:dict-type:list.query', 'GET', '/api/dict/type/list', '字典类型列表查询'),
    ('创建字典类型', 'system:dict-type:create.submit', 'POST', '/api/dict/type', '提交创建字典类型'),
    ('更新字典类型', 'system:dict-type:update.submit', 'PUT', '/api/dict/type/{id}', '提交更新字典类型'),
    ('删除字典类型', 'system:dict-type:delete.submit', 'DELETE', '/api/dict/type/{id}', '删除字典类型'),
    ('字典数据列表', 'system:dict-data:list.query', 'GET', '/api/dict/data/list', '字典数据列表查询'),
    ('按类型查询字典数据', 'system:dict-data:by-type.query', 'GET', '/api/dict/data/by-type/{dict_type}', '按字典类型查询字典数据'),
    ('全部字典数据', 'system:dict-data:all.query', 'GET', '/api/dict/all', '查询全部启用字典数据'),
    ('创建字典数据', 'system:dict-data:create.submit', 'POST', '/api/dict/data', '提交创建字典数据'),
    ('更新字典数据', 'system:dict-data:update.submit', 'PUT', '/api/dict/data/{id}', '提交更新字典数据'),
    ('删除字典数据', 'system:dict-data:delete.submit', 'DELETE', '/api/dict/data/{id}', '删除字典数据'),
    ('登录日志列表', 'system:login-log:list.query', 'GET', '/api/login-log/list', '登录日志列表查询'),
    ('操作日志列表', 'system:operation-log:list.query', 'GET', '/api/operation-log/list', '操作日志列表查询'),
    ('操作日志详情', 'system:operation-log:detail.query', 'GET', '/api/operation-log/{id}', '操作日志详情查询'),
    ('服务器监控', 'system:monitor:server.query', 'GET', '/api/monitor/server', '查询服务器监控信息'),
    ('缓存监控信息', 'system:monitor:cache-info.query', 'GET', '/api/monitor/cache/info', '查询缓存监控信息'),
    ('缓存键列表', 'system:monitor:cache-keys.query', 'GET', '/api/monitor/cache/keys', '查询缓存键列表'),
    ('缓存键详情', 'system:monitor:cache-key-detail.query', 'GET', '/api/monitor/cache/keys/{key}/value', '查询缓存键详情'),
    ('删除缓存键', 'system:monitor:cache-key-delete.submit', 'DELETE', '/api/monitor/cache/keys/{key}', '删除指定缓存键'),
    ('批量删除缓存键', 'system:monitor:cache-keys-delete.submit', 'DELETE', '/api/monitor/cache/keys', '按模式批量删除缓存键'),
    ('任务列表', 'scheduler:job:list.query', 'GET', '/api/job/list', '任务管理列表查询'),
    ('任务详情', 'scheduler:job:detail.query', 'GET', '/api/job/{id}', '任务详情查询'),
    ('创建任务', 'scheduler:job:create.submit', 'POST', '/api/job', '提交创建任务'),
    ('更新任务', 'scheduler:job:update.submit', 'PUT', '/api/job/{id}', '提交更新任务'),
    ('删除任务', 'scheduler:job:delete.submit', 'DELETE', '/api/job/{id}', '删除任务'),
    ('触发任务', 'scheduler:job:trigger.submit', 'POST', '/api/job/{id}/trigger', '手动触发任务'),
    ('启用任务', 'scheduler:job:enable.submit', 'PUT', '/api/job/{id}/enable', '启用任务'),
    ('停用任务', 'scheduler:job:disable.submit', 'PUT', '/api/job/{id}/disable', '停用任务'),
    ('任务执行记录', 'scheduler:run:list.query', 'GET', '/api/job/{id}/tasks', '查询任务执行记录'),
    ('任务最新执行历史', 'scheduler:run:latest-history.query', 'GET', '/api/job/{id}/latest-history', '查询任务最新执行历史'),
    ('按 Key 查询任务 ID', 'scheduler:job:query-id-by-key.query', 'GET', '/api/job/query-id-by-key', '按任务 Key 查询任务 ID'),
    ('按 Key 查询任务详情', 'scheduler:job:query-by-key.query', 'GET', '/api/job/query-by-key', '按任务 Key 查询任务详情'),
    ('任务命名空间列表', 'scheduler:job:namespaces.query', 'GET', '/api/job/namespaces', '查询任务命名空间列表'),
    ('任务应用列表', 'scheduler:job:apps.query', 'GET', '/api/job/apps', '查询任务应用列表'),
    ('文件列表', 'file:manage:list.query', 'GET', '/api/file/list', '文件管理列表查询'),
    ('文件详情', 'file:manage:detail.query', 'GET', '/api/file/{id}', '文件详情查询'),
    ('删除文件', 'file:manage:delete.submit', 'DELETE', '/api/file/{id}', '删除文件'),
    ('生成文件公开链接', 'file:manage:share.create', 'POST', '/api/file/{id}/public-link', '生成文件公开分享链接'),
    ('撤销文件公开链接', 'file:manage:share.delete', 'DELETE', '/api/file/{id}/public-link', '撤销文件公开分享链接'),
    ('更新文件可见性', 'file:manage:visibility.update', 'PUT', '/api/file/{id}/visibility', '更新文件可见性'),
    ('更新文件状态', 'file:manage:status.update', 'PUT', '/api/file/{id}/status', '更新文件状态'),
    ('更新文件展示名', 'file:manage:display-name.update', 'PUT', '/api/file/{id}/display-name', '更新文件展示名称'),
    ('移动文件', 'file:manage:move.update', 'PUT', '/api/file/{id}/move', '移动文件到目录'),
    ('代理上传文件', 'file:manage:upload.submit', 'POST', '/api/file/upload', '通过后端中转上传单个文件'),
    ('代理批量上传文件', 'file:manage:upload-batch.submit', 'POST', '/api/file/upload/batch', '通过后端中转批量上传文件'),
    ('生成预签名上传链接', 'file:manage:presign-upload.submit', 'POST', '/api/file/presign/upload', '生成预签名上传地址'),
    ('确认预签名上传', 'file:manage:presign-upload-callback.submit', 'POST', '/api/file/presign/upload/callback', '确认预签名上传并入库'),
    ('生成预签名下载链接', 'file:manage:presign-download.query', 'GET', '/api/file/{id}/presign/download', '生成预签名下载地址'),
    ('代理下载文件', 'file:manage:download.query', 'GET', '/api/file/{id}/download', '通过后端代理下载文件'),
    ('初始化分片上传', 'file:manage:multipart-init.submit', 'POST', '/api/file/multipart/init', '初始化分片上传'),
    ('查询分片上传进度', 'file:manage:multipart-parts.query', 'GET', '/api/file/multipart/parts', '查询已上传分片'),
    ('完成分片上传', 'file:manage:multipart-complete.submit', 'POST', '/api/file/multipart/complete', '完成分片上传'),
    ('取消分片上传', 'file:manage:multipart-abort.submit', 'POST', '/api/file/multipart/abort', '取消分片上传'),
    ('文件夹树', 'file:folder:tree.query', 'GET', '/api/file/folder/tree', '查询文件夹树'),
    ('文件夹详情', 'file:folder:detail.query', 'GET', '/api/file/folder/{id}', '查询文件夹详情'),
    ('创建文件夹', 'file:folder:create.submit', 'POST', '/api/file/folder', '提交创建文件夹'),
    ('更新文件夹', 'file:folder:update.submit', 'PUT', '/api/file/folder/{id}', '提交更新文件夹'),
    ('删除文件夹', 'file:folder:delete.submit', 'DELETE', '/api/file/folder/{id}', '删除文件夹'),
    ('资源列表', 'system:resource:list.query', 'GET', '/api/system/resource/list', '资源权限管理列表查询'),
    ('资源选项', 'system:resource:options.query', 'GET', '/api/system/resource/options', '查询资源下拉选项'),
    ('资源详情', 'system:resource:detail.query', 'GET', '/api/system/resource/{id}', '资源权限管理详情查询'),
    ('创建资源', 'system:resource:create.submit', 'POST', '/api/system/resource', '提交创建后端 API 资源'),
    ('更新资源', 'system:resource:update.submit', 'PUT', '/api/system/resource/{id}', '提交更新后端 API 资源'),
    ('启停资源', 'system:resource:enabled.submit', 'PUT', '/api/system/resource/{id}/enabled', '启用或停用后端 API 资源'),
    ('删除资源', 'system:resource:delete.submit', 'DELETE', '/api/system/resource/{id}', '删除后端 API 资源'),
    ('按钮资源绑定详情', 'system:resource:action-binding.query', 'GET', '/api/system/action-resource/action/{action_menu_id}', '查询按钮权限绑定的资源'),
    ('保存按钮资源绑定', 'system:resource:action-binding.submit', 'PUT', '/api/system/action-resource/action/{action_menu_id}', '保存按钮权限绑定的资源'),
    ('资源关联按钮', 'system:resource:resource-actions.query', 'GET', '/api/system/action-resource/resource/{resource_id}', '查询资源关联的按钮权限'),
    ('刷新资源权限策略', 'system:resource:reload.submit', 'POST', '/api/system/resource-permission/reload', '刷新内存中的资源权限策略')
) AS seed(resource_name, resource_code, method, path, description)
ON CONFLICT (resource_code) DO NOTHING;

INSERT INTO sys.action_resource (action_menu_id, resource_id)
SELECT m.id, r.id
FROM sys.menu m
JOIN (
    VALUES
        ('system:user:list', 'system:user:list.query'),
        ('system:user:create', 'system:user:create.submit'),
        ('system:user:create', 'system:role:list.query'),
        ('system:user:update', 'system:user:update.submit'),
        ('system:user:update', 'system:role:list.query'),
        ('system:user:delete', 'system:user:delete.submit'),
        ('system:user:reset-password', 'system:user:reset-password.submit'),
        ('system:user:detail', 'system:user:detail.query'),
        ('system:role:list', 'system:role:list.query'),
        ('system:role:create', 'system:role:create.submit'),
        ('system:role:update', 'system:role:list.query'),
        ('system:role:update', 'system:role:update.submit'),
        ('system:role:delete', 'system:role:delete.submit'),
        ('system:role:permission', 'system:role:permission.query'),
        ('system:role:permission', 'system:role:permission.submit'),
        ('system:role:permission', 'system:menu:list.query'),
        ('system:menu:list', 'system:menu:list.query'),
        ('system:menu:create', 'system:menu:list.query'),
        ('system:menu:create', 'system:menu:create.submit'),
        ('system:menu:update', 'system:menu:list.query'),
        ('system:menu:update', 'system:menu:update.submit'),
        ('system:menu:delete', 'system:menu:delete.submit'),
        ('system:button:create', 'system:menu:list.query'),
        ('system:button:create', 'system:button:create.submit'),
        ('system:button:update', 'system:menu:list.query'),
        ('system:button:update', 'system:button:update.submit'),
        ('system:resource:list', 'system:resource:list.query'),
        ('system:resource:list', 'system:resource:options.query'),
        ('system:resource:list', 'system:resource:resource-actions.query'),
        ('system:resource:create', 'system:resource:create.submit'),
        ('system:resource:update', 'system:resource:update.submit'),
        ('system:resource:update', 'system:resource:enabled.submit'),
        ('system:resource:delete', 'system:resource:delete.submit'),
        ('system:resource:bind', 'system:resource:list.query'),
        ('system:resource:bind', 'system:resource:options.query'),
        ('system:resource:bind', 'system:resource:action-binding.query'),
        ('system:resource:bind', 'system:resource:action-binding.submit'),
        ('system:resource:bind', 'system:resource:resource-actions.query'),
        ('system:resource:detail', 'system:resource:detail.query'),
        ('bind', 'system:resource:list.query'),
        ('bind', 'system:resource:options.query'),
        ('bind', 'system:resource:action-binding.query'),
        ('bind', 'system:resource:action-binding.submit'),
        ('bind', 'system:resource:resource-actions.query'),
        ('system:resource:reload', 'system:resource:reload.submit'),
        ('system:config:list', 'system:config:list.query'),
        ('system:config:list', 'system:config-group:list.query'),
        ('system:config:list', 'system:dict-type:list.query'),
        ('system:config:create', 'system:config:list.query'),
        ('system:config:create', 'system:config-group:list.query'),
        ('system:config:create', 'system:dict-type:list.query'),
        ('system:config:create', 'system:config:create.submit'),
        ('system:config:update', 'system:config-group:list.query'),
        ('system:config:update', 'system:dict-type:list.query'),
        ('system:config:update', 'system:config:update.submit'),
        ('system:config:delete', 'system:config:delete.submit'),
        ('system:config:detail', 'system:config:detail.query'),
        ('system:config-group:list', 'system:config-group:list.query'),
        ('system:config-group:create', 'system:config-group:create.submit'),
        ('system:config-group:update', 'system:config-group:update.submit'),
        ('system:config-group:delete', 'system:config-group:delete.submit'),
        ('system:config-group:detail', 'system:config-group:detail.query'),
        ('system:notice:list', 'system:notice:list.query'),
        ('system:notice:create', 'system:notice:create.submit'),
        ('system:notice:create', 'system:role:list.query'),
        ('system:notice:create', 'system:user:list.query'),
        ('system:notice:detail', 'system:notice:detail.query'),
        ('system:notice:update', 'system:notice:update.submit'),
        ('system:notice:update', 'system:role:list.query'),
        ('system:notice:update', 'system:user:list.query'),
        ('system:notice:delete', 'system:notice:delete.submit'),
        ('system:notice:publish', 'system:notice:publish.submit'),
        ('system:notice:revoke', 'system:notice:revoke.submit'),
        ('system:notice:pin', 'system:notice:pin.submit'),
        ('system:notice:pin', 'system:notice:unpin.submit'),
        ('system:user-notice:list', 'system:user-notice:list.query'),
        ('system:user-notice:list', 'system:user-notice:latest.query'),
        ('system:user-notice:list', 'system:user-notice:unread-count.query'),
        ('system:user-notice:detail', 'system:user-notice:detail.query'),
        ('system:user-notice:read', 'system:user-notice:read.submit'),
        ('system:user-notice:read', 'system:user-notice:read-all.submit'),
        ('system:online:list', 'system:online:list.query'),
        ('system:online:kick', 'system:online:kick.submit'),
        ('system:online:kick', 'system:online:kick-device.submit'),
        ('system:dict-type:list', 'system:dict-type:list.query'),
        ('system:dict-type:create', 'system:dict-type:create.submit'),
        ('system:dict-type:update', 'system:dict-type:list.query'),
        ('system:dict-type:update', 'system:dict-type:update.submit'),
        ('system:dict-type:delete', 'system:dict-type:delete.submit'),
        ('system:dict-data:list', 'system:dict-data:list.query'),
        ('system:dict-data:list', 'system:dict-data:by-type.query'),
        ('system:dict-data:list', 'system:dict-data:all.query'),
        ('system:dict-data:create', 'system:dict-type:list.query'),
        ('system:dict-data:create', 'system:dict-data:create.submit'),
        ('system:dict-data:update', 'system:dict-type:list.query'),
        ('system:dict-data:update', 'system:dict-data:update.submit'),
        ('system:dict-data:delete', 'system:dict-data:delete.submit'),
        ('system:login-log:list', 'system:login-log:list.query'),
        ('system:operation-log:list', 'system:operation-log:list.query'),
        ('system:operation-log:detail', 'system:operation-log:detail.query'),
        ('system:monitor:server', 'system:monitor:server.query'),
        ('system:monitor:cache', 'system:monitor:cache-info.query'),
        ('system:monitor:cache', 'system:monitor:cache-keys.query'),
        ('system:monitor:cache', 'system:monitor:cache-key-detail.query'),
        ('system:monitor:cache-delete', 'system:monitor:cache-key-delete.submit'),
        ('system:monitor:cache-delete', 'system:monitor:cache-keys-delete.submit'),
        ('scheduler:job:list', 'scheduler:job:list.query'),
        ('scheduler:job:list', 'scheduler:job:namespaces.query'),
        ('scheduler:job:list', 'scheduler:job:apps.query'),
        ('scheduler:job:detail', 'scheduler:job:detail.query'),
        ('scheduler:job:create', 'scheduler:job:create.submit'),
        ('scheduler:job:create', 'scheduler:job:namespaces.query'),
        ('scheduler:job:create', 'scheduler:job:apps.query'),
        ('scheduler:job:update', 'scheduler:job:update.submit'),
        ('scheduler:job:update', 'scheduler:job:namespaces.query'),
        ('scheduler:job:update', 'scheduler:job:apps.query'),
        ('scheduler:job:toggle', 'scheduler:job:enable.submit'),
        ('scheduler:job:toggle', 'scheduler:job:disable.submit'),
        ('scheduler:job:trigger', 'scheduler:job:trigger.submit'),
        ('scheduler:job:delete', 'scheduler:job:delete.submit'),
        ('scheduler:run:list', 'scheduler:job:list.query'),
        ('scheduler:run:list', 'scheduler:run:list.query'),
        ('scheduler:run:list', 'scheduler:run:latest-history.query'),
        ('scheduler:run:detail', 'scheduler:run:list.query'),
        ('scheduler:run:detail', 'scheduler:run:latest-history.query'),
        ('file:manage:list', 'file:manage:list.query'),
        ('file:manage:list', 'file:folder:tree.query'),
        ('file:manage:upload', 'file:manage:upload.submit'),
        ('file:manage:upload', 'file:manage:upload-batch.submit'),
        ('file:manage:upload', 'file:manage:presign-upload.submit'),
        ('file:manage:upload', 'file:manage:presign-upload-callback.submit'),
        ('file:manage:upload', 'file:manage:multipart-init.submit'),
        ('file:manage:upload', 'file:manage:multipart-parts.query'),
        ('file:manage:upload', 'file:manage:multipart-complete.submit'),
        ('file:manage:upload', 'file:manage:multipart-abort.submit'),
        ('file:manage:upload', 'file:folder:tree.query'),
        ('file:manage:detail', 'file:manage:detail.query'),
        ('file:manage:update', 'file:folder:tree.query'),
        ('file:manage:update', 'file:manage:display-name.update'),
        ('file:manage:update', 'file:manage:move.update'),
        ('file:manage:update', 'file:manage:visibility.update'),
        ('file:manage:update', 'file:manage:status.update'),
        ('file:manage:delete', 'file:manage:delete.submit'),
        ('file:manage:share', 'file:manage:share.create'),
        ('file:manage:share', 'file:manage:share.delete'),
        ('file:folder:tree', 'file:folder:tree.query'),
        ('file:folder:detail', 'file:folder:detail.query'),
        ('file:folder:create', 'file:folder:tree.query'),
        ('file:folder:create', 'file:folder:create.submit'),
        ('file:folder:update', 'file:folder:tree.query'),
        ('file:folder:update', 'file:folder:update.submit'),
        ('file:folder:delete', 'file:folder:delete.submit'),
        ('file:manage:download', 'file:manage:presign-download.query'),
        ('file:manage:download', 'file:manage:download.query')
) AS mapping(auth_mark, resource_code) ON mapping.auth_mark = m.auth_mark
JOIN sys.resource r ON r.resource_code = mapping.resource_code
WHERE m.menu_type = 2
ON CONFLICT (action_menu_id, resource_id) DO NOTHING;
