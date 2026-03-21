# 多时区支持迁移指南

## 修改内容

### 1. 代码层修改 ✅

**配置文件**：
- `config/app-dev.toml` - 数据库时区改为 UTC

**实体文件**：
- `sys_user.rs` - 使用 `chrono::Utc::now().naive_utc()`
- `sys_role.rs` - 使用 `chrono::Utc::now().naive_utc()`
- `sys_menu.rs` - 使用 `chrono::Utc::now().naive_utc()`

### 2. 数据库迁移（需要手动执行）

**执行迁移脚本**：
```bash
psql -U admin -d summerrs-admin -f sql/migration/migrate_to_utc.sql
```

**迁移说明**：
- 将 `timestamp` 改为 `timestamptz`（带时区）
- 现有数据按 `Asia/Shanghai` 时区转换为 UTC
- 例如：`2026-02-28 04:10:16` (上海) → `2026-02-27 20:10:16+00` (UTC)

### 3. 时间处理流程

**存储**：
```
应用服务器
  ↓ chrono::Utc::now()
获取 UTC 时间：2026-02-28 04:10:16 UTC
  ↓ SeaORM
存储到 PostgreSQL (timestamptz)
  ↓
数据库：2026-02-28 04:10:16+00
```

**读取**：
```
数据库：2026-02-28 04:10:16+00 (UTC)
  ↓ SeaORM
应用：NaiveDateTime (2026-02-28 04:10:16)
  ↓ API 响应
前端：2026-02-28T04:10:16.000Z (ISO 8601)
  ↓ JavaScript Date
用户看到：根据浏览器时区自动转换
  - 上海用户：2026-02-28 12:10:16
  - 纽约用户：2026-02-27 23:10:16
```

### 4. 前端处理

**后端返回格式**（当前）：
```json
{
  "createTime": "2026-02-28 04:10:16"
}
```

**前端处理**：
```javascript
// 方式 1：假设后端返回的是 UTC 时间
const utcDate = new Date(createTime + 'Z'); // 添加 Z 表示 UTC
console.log(utcDate.toLocaleString()); // 自动转换为本地时区

// 方式 2：使用 moment.js 或 dayjs
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';
import timezone from 'dayjs/plugin/timezone';

dayjs.extend(utc);
dayjs.extend(timezone);

const localTime = dayjs.utc(createTime).local().format('YYYY-MM-DD HH:mm:ss');
```

### 5. 验证步骤

**1. 执行数据库迁移**：
```bash
psql -U admin -d summerrs-admin -f sql/migration/migrate_to_utc.sql
```

**2. 重启应用**：
```bash
cargo run
```

**3. 测试创建记录**：
```bash
# 创建一条新记录
curl -X POST http://localhost:8080/api/system/menu \
  -H "Content-Type: application/json" \
  -d '{"name":"test","path":"/test","title":"测试"}'
```

**4. 检查数据库时间**：
```sql
SELECT id, title, create_time,
       create_time AT TIME ZONE 'UTC' as utc_time,
       create_time AT TIME ZONE 'Asia/Shanghai' as shanghai_time
FROM sys.menu
ORDER BY id DESC
LIMIT 1;
```

应该看到：
- `create_time`: 带 `+00` 后缀（UTC）
- `utc_time`: UTC 时间
- `shanghai_time`: 上海时间（UTC+8）

### 6. 注意事项

⚠️ **重要**：
1. 执行迁移前务必备份数据库
2. 迁移会将现有时间从上海时区转换为 UTC
3. 前端需要相应调整时间显示逻辑
4. 建议在测试环境先验证

✅ **优点**：
- 支持全球多时区用户
- 避免夏令时问题
- 数据一致性更好
- 符合国际标准

### 7. 回滚方案

如果需要回滚到本地时区：

```sql
-- 回滚到 timestamp without time zone
ALTER TABLE sys."user"
  ALTER COLUMN create_time TYPE timestamp USING create_time AT TIME ZONE 'Asia/Shanghai',
  ALTER COLUMN update_time TYPE timestamp USING update_time AT TIME ZONE 'Asia/Shanghai';

-- 同样处理其他表...
```

然后修改代码和配置回到 `chrono::Local` 和 `TimeZone=Asia/Shanghai`。
