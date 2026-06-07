-- ============================================================
-- 自动生成的菜单数据
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1100, 0, 1, 'File', '/file', '/index/index', '', 'ri:folder-3-line', 'menus.file.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1101, 1100, 1, 'FileManage', 'manage', '/file/manage', '', 'ri:file-list-3-line', 'menus.file.manage', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1102, 1100, 1, 'FileExamples', 'examples', '/file/examples', '', 'ri:upload-cloud-2-line', 'menus.file.examples', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1103, 1101, 2, '', '', '', '', '', '查询文件', '', false, false, false, false, false, false, false, false, '', '', '查询文件', 'file:manage:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1104, 1101, 2, '', '', '', '', '', '上传文件', '', false, false, false, false, false, false, false, false, '', '', '上传文件', 'file:manage:upload', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1105, 1101, 2, '', '', '', '', '', '编辑文件', '', false, false, false, false, false, false, false, false, '', '', '编辑文件', 'file:manage:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1106, 1101, 2, '', '', '', '', '', '删除文件', '', false, false, false, false, false, false, false, false, '', '', '删除文件', 'file:manage:delete', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1107, 1101, 2, '', '', '', '', '', '分享文件', '', false, false, false, false, false, false, false, false, '', '', '分享文件', 'file:manage:share', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1108, 1101, 2, '', '', '', '', '', '新增目录', '', false, false, false, false, false, false, false, false, '', '', '新增目录', 'file:folder:create', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1109, 1101, 2, '', '', '', '', '', '编辑目录', '', false, false, false, false, false, false, false, false, '', '', '编辑目录', 'file:folder:update', 7, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1110, 1101, 2, '', '', '', '', '', '删除目录', '', false, false, false, false, false, false, false, false, '', '', '删除目录', 'file:folder:delete', 8, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1111, 1101, 2, '', '', '', '', '', '下载文件', '', false, false, false, false, false, false, false, false, '', '', '下载文件', 'file:manage:download', 9, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

-- 重置序列
SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu));
