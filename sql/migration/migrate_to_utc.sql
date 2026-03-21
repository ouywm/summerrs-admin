-- 迁移到 UTC 时区支持
-- 执行前请备份数据库！

-- 1. 修改表字段类型为 timestamptz（带时区）
ALTER TABLE sys."user"
  ALTER COLUMN create_time TYPE timestamptz USING create_time AT TIME ZONE 'Asia/Shanghai',
  ALTER COLUMN update_time TYPE timestamptz USING update_time AT TIME ZONE 'Asia/Shanghai';

ALTER TABLE sys."role"
  ALTER COLUMN create_time TYPE timestamptz USING create_time AT TIME ZONE 'Asia/Shanghai',
  ALTER COLUMN update_time TYPE timestamptz USING update_time AT TIME ZONE 'Asia/Shanghai';

ALTER TABLE sys.menu
  ALTER COLUMN create_time TYPE timestamptz USING create_time AT TIME ZONE 'Asia/Shanghai',
  ALTER COLUMN update_time TYPE timestamptz USING update_time AT TIME ZONE 'Asia/Shanghai';

-- 2. 验证数据
SELECT 'sys.user' AS table_name, COUNT(*) AS count FROM sys."user"
UNION ALL
SELECT 'sys.role', COUNT(*) FROM sys."role"
UNION ALL
SELECT 'sys.menu', COUNT(*) FROM sys.menu;

-- 3. 查看时间示例（应该显示带时区的时间）
SELECT
  'sys.user' AS table_name,
  create_time,
  create_time AT TIME ZONE 'UTC' AS utc_time,
  create_time AT TIME ZONE 'Asia/Shanghai' AS shanghai_time
FROM sys."user"
LIMIT 1;
