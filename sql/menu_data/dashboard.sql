-- ============================================================
-- 自动生成的菜单数据
-- ============================================================

INSERT INTO sys_menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1, 0, 1, 'Dashboard', '/dashboard', '/index/index', '', 'ri:pie-chart-line', 'menus.dashboard.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys_menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (2, 1, 1, 'Console', 'console', '/dashboard/console', '', 'ri:home-smile-2-line', 'menus.dashboard.console', '', false, false, false, false, false, false, true, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys_menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (3, 1, 1, 'Analysis', 'analysis', '/dashboard/analysis', '', 'ri:align-item-bottom-line', 'menus.dashboard.analysis', '', false, false, false, false, false, false, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys_menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (4, 1, 1, 'Ecommerce', 'ecommerce', '/dashboard/ecommerce', '', 'ri:bar-chart-box-line', 'menus.dashboard.ecommerce', '', false, false, false, false, false, false, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

-- 重置序列
SELECT setval('sys_menu_id_seq', (SELECT MAX(id) FROM sys_menu));
