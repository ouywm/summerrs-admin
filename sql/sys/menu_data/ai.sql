-- ============================================================
-- AI 模块菜单已迁移到开发工具
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

-- AI 代码生成器菜单见 sql/sys/menu_data/dev_tools.sql。

-- 重置序列
SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu))
WHERE EXISTS (SELECT 1 FROM sys.menu);
