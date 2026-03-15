-- 权限位图迁移：为 sys_menu 添加 bit_position 列
ALTER TABLE sys_menu ADD COLUMN bit_position INTEGER;

-- 为现有按钮权限分配位置（按 ID 顺序，从 0 开始）
WITH ranked AS (
  SELECT id, ROW_NUMBER() OVER (ORDER BY id) - 1 AS pos
  FROM sys_menu WHERE menu_type = 2 AND auth_mark != '' AND enabled = true
)
UPDATE sys_menu SET bit_position = ranked.pos
FROM ranked WHERE sys_menu.id = ranked.id;
