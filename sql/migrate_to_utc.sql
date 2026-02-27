-- 迁移到 UTC 时区支持
-- 执行前请备份数据库！

-- 1. 修改表字段类型为 timestamptz（带时区）
ALTER TABLE sys_user
  ALTER COLUMN create_time TYPE timestamptz USING create_time AT TIME ZONE 'Asia/Shanghai',
  ALTER COLUMN update_time TYPE timestamptz USING update_time AT TIME ZONE 'Asia/Shanghai';

ALTER TABLE sys_role
  ALTER COLUMN create_time TYPE timestamptz USING create_time AT TIME ZONE 'Asia/Shanghai',
  ALTER COLUMN update_time TYPE timestamptz USING update_time AT TIME ZONE 'Asia/Shanghai';

ALTER TABLE sys_menu
  ALTER COLUMN create_time TYPE timestamptz USING create_time AT TIME ZONE 'Asia/Shanghai',
  ALTER COLUMN update_time TYPE timestamptz USING update_time AT TIME ZONE 'Asia/Shanghai';

-- 2. 验证数据
SELECT 'sys_user' as table_name, COUNT(*) as count FROM sys_user
UNION ALL
SELECT 'sys_role', COUNT(*) FROM sys_role
UNION ALL
SELECT 'sys_menu', COUNT(*) FROM sys_menu;

-- 3. 查看时间示例（应该显示带时区的时间）
SELECT
  'sys_user' as table_name,
  create_time,
  create_time AT TIME ZONE 'UTC' as utc_time,
  create_time AT TIME ZONE 'Asia/Shanghai' as shanghai_time
FROM sys_user
LIMIT 1;
