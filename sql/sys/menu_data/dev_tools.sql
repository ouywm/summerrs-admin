-- ============================================================
-- 开发工具模块（ID: 1300-1308）
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1300, 0, 1, 'DevTools', '/dev-tools', '/index/index', '', 'ri:tools-line', 'menus.devTools.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 90, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1301, 1300, 1, 'DevToolsAiGenerator', 'ai-generator', '/dev-tools/ai-generator', '', 'ri:sparkling-2-line', 'menus.devTools.aiGenerator', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1302, 1301, 2, '', '', '', '', '', '查询生成历史', '', false, false, false, false, false, false, false, false, '', '', '查询生成历史', 'ai:codegen:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1303, 1301, 2, '', '', '', '', '', '创建生成会话', '', false, false, false, false, false, false, false, false, '', '', '创建生成会话', 'ai:codegen:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1304, 1301, 2, '', '', '', '', '', '解析需求', '', false, false, false, false, false, false, false, false, '', '', '解析需求', 'ai:codegen:analyze', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1305, 1301, 2, '', '', '', '', '', '更新Schema', '', false, false, false, false, false, false, false, false, '', '', '更新Schema', 'ai:codegen:update-schema', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1306, 1301, 2, '', '', '', '', '', '校验Schema', '', false, false, false, false, false, false, false, false, '', '', '校验Schema', 'ai:codegen:validate', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1307, 1301, 2, '', '', '', '', '', '生成预览', '', false, false, false, false, false, false, false, false, '', '', '生成预览', 'ai:codegen:preview', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1308, 1301, 2, '', '', '', '', '', '执行生成', '', false, false, false, false, false, false, false, false, '', '', '执行生成', 'ai:codegen:apply', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

-- 重置序列
SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu));
