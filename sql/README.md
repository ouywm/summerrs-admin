# SQL Layout

更新时间：2026-03-21

当前 SQL 已按业务域拆目录，也是仓库内数据库结构的 source of truth：

- `sql/sys/`：系统管理、后台账号、菜单、日志、字典、通知、文件、认证层
- `sql/tenant/`：租户控制面（租户主表、租户数据源/隔离元数据、租户成员关系等）
- `sql/biz/`：B/C 端业务账号与业务域
- `sql/ai/`：AI relay / gateway / control-plane
  - 已按业务域继续拆为 `routing/`、`requests/`、`billing/`、`tenancy/`、`governance/`、`guardrails/`、`storage/`、`platform/`、`operations/`
- `sql/migration/`：一次性迁移、修复、时区调整脚本

补充目录：

- `sql/sys/menu_data/`：系统菜单种子数据与路由转 SQL 辅助文件
- `sql/sys/menu_data_all.sql`：菜单合并 SQL 产物

命名约定：

- SQL 文件名也去掉重复域前缀，例如 `sql/ai/request.sql`、`sql/sys/user.sql`、`sql/biz/user.sql`
- 物理表名不再重复域前缀，直接落在对应 schema，例如 `ai.request`、`sys.notice`、`biz.customer`
- 只有少量 PostgreSQL 保留字会保留引号：`sys."user"`、`sys."role"`、`biz."user"`、`biz."role"`、`ai."order"`、`ai."transaction"`

推荐执行顺序：

1. `sql/sys/`
2. `sql/tenant/`
3. `sql/biz/`
4. `sql/ai/`（先看 `sql/ai/README.md` 再进入子目录）
5. 按需导入 `sql/sys/menu_data_all.sql`
6. 仅在老库改造时执行 `sql/migration/`

关于 PostgreSQL schema：

- 当前方案已经确定使用 `sys` / `tenant` / `biz` / `ai` 物理 schema
- `public` 只建议保留 `seaql_migrations`、扩展对象和少量非业务公共对象
- 老库从 `public.sys_*` / `public.biz_*` / `public.ai_*` 迁移时，统一通过 `sql/migration/20260321_split_public_to_sys_biz_ai_schema.sql` 完成“迁移到 schema + 去掉重复前缀”
