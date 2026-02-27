-- ============================================================
-- 自动生成的菜单数据
-- ============================================================

INSERT INTO sys_menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (800, 0, 1, 'Safeguard', '/safeguard', '/index/index', '', 'ri:shield-check-line', 'menus.safeguard.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys_menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (801, 800, 1, 'SafeguardServer', 'server', '/safeguard/server', '', 'ri:hard-drive-3-line', 'menus.safeguard.server', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

-- 重置序列
SELECT setval('sys_menu_id_seq', (SELECT MAX(id) FROM sys_menu));
