# MCP And Generator Patterns

这部分只讲“在本仓库里如何用 MCP 干活”，重点是：先发现 schema，再生成 raw 代码，再把稳定逻辑放到正确的扩展层。

## Canonical examples

- MCP 插件入口：`crates/summer-mcp/src/plugin.rs`
- runtime / server：`crates/summer-mcp/src/runtime.rs`、`crates/summer-mcp/src/server.rs`
- 生成器实现：`crates/summer-mcp/src/tools/*`
- table tools 路由：`crates/summer-mcp/src/table_tools/router.rs`

## 什么时候优先 MCP

以下需求优先 MCP，而不是先手写：

- 已有表，要补 raw entity
- 要快速补后台 CRUD 骨架
- 要生成前端 API / type / page
- 要补菜单或字典草案

## 推荐顺序

1. `schema_list_tables` / `schema://tables`
2. `schema_describe_table` / `schema://table/{table}`
3. `generate_entity_from_table`
4. `generate_admin_module_from_table`
5. `generate_frontend_bundle_from_table`
6. `menu_tool` / `dict_tool`

不要跳过 schema discovery 直接猜字段。

## 本仓库 MCP 的关键能力

### schema discovery

- `schema_list_tables`
- `schema_describe_table`
- `schema://tables`
- `schema://table/{table}`

### generic table CRUD

- `table_get`
- `table_query`
- `table_insert`
- `table_update`
- `table_delete`

### SQL escape hatches

- `sql_query_readonly`
- `sql_exec`

### generators

- `generate_entity_from_table`
- `upgrade_entity_enums_from_table`
- `generate_admin_module_from_table`
- `generate_frontend_api_from_table`
- `generate_frontend_page_from_table`
- `generate_frontend_bundle_from_table`

### business tools

- `menu_tool`
- `dict_tool`

## 生成后的目标位置

### system 模块常见落点

- raw entity：`crates/summer-system-model/src/entity_gen`
- entity 扩展层：`crates/summer-system-model/src/entity`
- DTO：`crates/summer-system-model/src/dto`
- VO：`crates/summer-system-model/src/vo`
- route：`crates/summer-system/src/router`
- service：`crates/summer-system/src/service`

### 重要规则

- 生成器产物优先进入 `entity_gen`
- 稳定逻辑不要补进 raw entity，放 `entity` 扩展层
- 应用代码只依赖 `entity`

如果工具默认输出到别的目录，先生成，再手动归位到这套结构。

## 后端 CRUD 生成

当目标是“已有表 -> admin 模块”时：

1. 确认表结构
2. 生成 raw entity
3. 生成 admin module
4. 再按业务补 service / 权限 / 日志 / 菜单 / 字典

不适合纯生成解决的需求：

- 登录、鉴权
- 多表聚合接口
- 在线用户、踢下线
- Socket.IO 事件
- 特殊状态流转

## 菜单和字典怎么落库

### 菜单

不要直接写 `sys.menu`。

优先：

- `menu_tool.plan_config`
- `menu_tool.export_config`
- `menu_tool.apply_config`

### 字典

不要直接写 `sys.dict_type` / `sys.dict_data`。

优先：

- `dict_tool.plan_bundle`
- `dict_tool.export_bundle`
- `dict_tool.apply_bundle`

## 默认验证

- MCP 改动：`cargo test -p summer-mcp`
- 大改动：`cargo test --workspace --quiet`
