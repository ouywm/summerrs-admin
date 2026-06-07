-- ============================================================
-- 自动生成的菜单数据
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1000, 0, 1, 'Scheduler', '/scheduler', '/index/index', '', 'ri:timer-line', 'menus.scheduler.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1001, 1000, 1, 'SchedulerJob', 'job', '/scheduler/job', '', 'ri:list-check-2', 'menus.scheduler.job', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1002, 1000, 1, 'SchedulerRun', 'run', '/scheduler/run', '', 'ri:history-line', 'menus.scheduler.run', '', false, false, false, false, false, true, false, false, '', '', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1003, 1001, 2, '', '', '', '', '', '查询任务', '', false, false, false, false, false, false, false, false, '', '', '查询任务', 'scheduler:job:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1004, 1001, 2, '', '', '', '', '', '新增任务', '', false, false, false, false, false, false, false, false, '', '', '新增任务', 'scheduler:job:create', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1005, 1001, 2, '', '', '', '', '', '编辑任务', '', false, false, false, false, false, false, false, false, '', '', '编辑任务', 'scheduler:job:update', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1006, 1001, 2, '', '', '', '', '', '启停任务', '', false, false, false, false, false, false, false, false, '', '', '启停任务', 'scheduler:job:toggle', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1007, 1001, 2, '', '', '', '', '', '触发任务', '', false, false, false, false, false, false, false, false, '', '', '触发任务', 'scheduler:job:trigger', 5, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1008, 1001, 2, '', '', '', '', '', '删除任务', '', false, false, false, false, false, false, false, false, '', '', '删除任务', 'scheduler:job:delete', 6, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1009, 1002, 2, '', '', '', '', '', '查询执行记录', '', false, false, false, false, false, false, false, false, '', '', '查询执行记录', 'scheduler:run:list', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (1010, 1002, 2, '', '', '', '', '', '查看执行记录', '', false, false, false, false, false, false, false, false, '', '', '查看执行记录', 'scheduler:run:detail', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

-- 重置序列
SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu));
