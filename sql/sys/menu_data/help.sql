-- ============================================================
-- 自动生成的菜单数据
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (900, 0, 1, 'Document', '', '', '', 'ri:bill-line', 'menus.help.document', 'https://docs.example.com', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (901, 0, 1, 'LiteVersion', '', '', '', 'ri:bus-2-line', 'menus.help.liteVersion', 'https://lite.example.com', false, false, false, false, false, false, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (902, 0, 1, 'OldVersion', '', '', '', 'ri:subway-line', 'menus.help.oldVersion', 'https://old.example.com', false, false, false, false, false, false, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (903, 0, 1, 'ChangeLog', '/change/log', '/change/log', '', 'ri:gamepad-line', 'menus.plan.log', '', false, false, false, false, false, false, false, false, 'v3.0.1', '', '', '', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

-- 重置序列
SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu));
