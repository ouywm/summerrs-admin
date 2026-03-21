-- ============================================================
-- 自动生成的菜单数据
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (200, 0, 1, 'Article', '/article', '/index/index', '', 'ri:book-2-line', 'menus.article.title', '', false, false, false, false, false, false, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (201, 200, 1, 'ArticleList', 'article-list', '/article/list', '', 'ri:article-line', 'menus.article.articleList', '', false, false, false, false, false, true, false, false, '', '', '', '', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (202, 201, 2, '', '', '', '', '', '新增', '', false, false, false, false, false, false, false, false, '', '', '新增', 'add', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (203, 201, 2, '', '', '', '', '', '编辑', '', false, false, false, false, false, false, false, false, '', '', '编辑', 'edit', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (204, 200, 1, 'ArticleDetail', 'detail/:id', '/article/detail', '', '', 'menus.article.articleDetail', '', false, true, false, false, false, true, false, false, '', '/article/article-list', '', '', 2, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (205, 200, 1, 'ArticleComment', 'comment', '/article/comment', '', 'ri:mail-line', 'menus.article.comment', '', false, false, false, false, false, true, false, false, '', '', '', '', 3, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (206, 200, 1, 'ArticlePublish', 'publish', '/article/publish', '', 'ri:telegram-2-line', 'menus.article.articlePublish', '', false, false, false, false, false, true, false, false, '', '', '', '', 4, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) VALUES (207, 206, 2, '', '', '', '', '', '发布', '', false, false, false, false, false, false, false, false, '', '', '发布', 'add', 1, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

-- 重置序列
SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu));
