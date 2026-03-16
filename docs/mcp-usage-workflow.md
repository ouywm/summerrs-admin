# MCP 使用流程说明

## 目标

这份文档说明 `summerrs_admin` 这个 MCP Server 的推荐使用方式。

重点不是“有哪些 tool”，而是“AI 应该按什么顺序调用”，这样才能稳定完成：

- 表结构发现
- 数据读取与修改
- 后端代码生成
- 前端代码生成
- 菜单与字典配置落库
- 最终验证

---

## 一、基本原则

- 先读资源，再调工具。不要先猜表名、字段名、主键、枚举值。
- 简单 CRUD 优先走通用表工具，不要一上来写 SQL。
- 菜单和字典优先走业务 tool，不要直接写 `sys_menu` / `sys_dict_type` / `sys_dict_data`。
- 生成代码和落业务配置是两件事。`generate_frontend_bundle_from_table` 只生成代码和草案，不会自动写菜单、字典。
- 先生成到临时目录检查，再决定是否落到真实项目目录。
- `sql_query_readonly` 是复杂查询逃生口，`sql_exec` 是显式 DDL/DML 逃生口，不是默认主路径。

---

## 二、推荐总流程

推荐顺序如下：

1. 发现表结构
2. 读取样例数据
3. 生成 Entity
4. 生成后端 CRUD 骨架
5. 生成前端 bundle
6. 审查生成结果中的 `menu_config_draft` 和 `dict_bundle_drafts`
7. 用 `menu_tool` / `dict_tool` 做 plan/export/apply
8. 回查菜单树、字典数据、页面文件和接口

如果客户端支持 MCP prompts，可以直接先取这三个流程提示：

- `discover_table_workflow`
- `generate_crud_bundle_workflow`
- `rollout_menu_dict_workflow`

---

## 三、第一步：先发现表结构

### 资源入口

优先使用 MCP resources：

- `schema://tables`
- `schema://table/{table}`

对应能力：

- `schema://tables`：返回当前数据库暴露的表列表
- `schema://table/{table}`：返回指定表的字段、主键、可写性、隐藏字段、枚举值等

### Tool 入口

如果客户端更适合调 tool，也可以用：

- `schema_list_tables`
- `schema_describe_table`

### 推荐做法

- 先读 `schema://tables`，确认目标表是否存在
- 再读 `schema://table/{table}`，确认：
  - 主键字段
  - 哪些字段可读
  - 哪些字段允许 create/update
  - 是否有枚举值
  - 默认值、注释、可空信息

### 目的

这一步是为了避免 AI 猜错字段，尤其是：

- 主键不是 `id`
- 字段不可更新
- 隐藏字段不能直接返回
- 状态字段实际是枚举或字典
- 时间字段是 `date` 还是 `timestamp`

---

## 四、第二步：数据读取与修改怎么选工具

## 4.1 简单读取

优先使用：

- `table_get`
- `table_query`

适合场景：

- 按主键查一条
- 按条件查列表
- 分页读取
- 指定列返回

## 4.2 简单写入

优先使用：

- `table_insert`
- `table_update`
- `table_delete`

适合场景：

- 单表单行新增
- 单表按主键更新
- 单表按主键删除

## 4.3 复杂查询

使用：

- `sql_query_readonly`

适合场景：

- 多表 join
- 聚合统计
- `WITH` 查询
- `table_query` 表达不了的复杂读取

注意：

- 这个 tool 只用于读
- 不要拿它做写操作

## 4.4 显式 SQL 执行

使用：

- `sql_exec`

适合场景：

- `CREATE TABLE`
- `ALTER TABLE`
- `CREATE INDEX`
- 数据修复
- 无法用表工具表达的显式 DML/DDL

注意：

- 不要把 `sql_exec` 当默认入口
- 能用表工具表达的简单增删改，不建议退回裸 SQL

---

## 五、第三步：代码生成推荐顺序

推荐顺序固定为：

1. `generate_entity_from_table`
2. `generate_admin_module_from_table`
3. `generate_frontend_bundle_from_table`

原因很简单：

- Entity 是后续生成上下文的基础
- 后端 CRUD 骨架依赖表结构和命名约定
- 前端 bundle 要和生成出的 API / TS 类型保持一致

## 5.1 生成 Entity

使用：

```json
{
  "table": "sys_user",
  "output_dir": "/tmp/mcp-preview/entity",
  "overwrite": true
}
```

说明：

- 这个 tool 调的是 `sea-orm-cli`
- 它负责把数据库表同步成 SeaORM entity
- 推荐先输出到 `/tmp` 看结果，再决定是否写回正式项目

## 5.2 生成后端 CRUD 骨架

使用：

```json
{
  "table": "sys_user",
  "route_base": "user",
  "output_dir": "/tmp/mcp-preview/backend",
  "overwrite": true
}
```

生成内容通常包括：

- router
- service
- dto
- vo

## 5.3 生成前端 bundle

最推荐直接使用：

- `generate_frontend_bundle_from_table`

示例：

```json
{
  "table": "sys_user",
  "route_base": "user",
  "output_dir": "/tmp/mcp-preview/frontend",
  "overwrite": true
}
```

如果目标是 Art Design Pro，需要显式指定：

```json
{
  "table": "sys_user",
  "route_base": "user",
  "target_preset": "art_design_pro",
  "output_dir": "/Volumes/990pro/code/vue/art-design-pro",
  "overwrite": true
}
```

这个 tool 会一次生成：

- API 文件
- TS 类型文件
- 页面文件

同时返回：

- `required_dict_types`
- `dict_bundle_drafts`
- `menu_config_draft`

这三个返回值很关键，它们是后续菜单/字典落库的桥。

---

## 六、第四步：菜单和字典不要手写 SQL

前端 bundle 生成完成后，不代表页面已经能在系统里被菜单加载，也不代表字典已经入库。

还差两件事：

- 菜单配置落库
- 字典配置落库

这时不要自己拼 `INSERT INTO sys_menu ...` 或 `INSERT INTO sys_dict_data ...`。

推荐统一走：

- `menu_tool`
- `dict_tool`

---

## 七、菜单流程

`menu_tool` 是业务入口，调用时必须带 `action`。

支持的主要动作：

- `list_tree`
- `get_user_tree`
- `plan_config`
- `export_config`
- `apply_config`
- `create_menu`
- `create_button`
- `update_menu`
- `update_button`
- `delete_node`

### 推荐顺序

1. `list_tree`
2. 基于生成器返回的 `menu_config_draft` 做必要修正
3. `plan_config`
4. 如需落地文件审查，用 `export_config`
5. 确认后再 `apply_config`
6. 再次 `list_tree` 验证

### 推荐原因

- `list_tree` 可以先看当前已有菜单和排序
- `plan_config` 只预演，不写库
- `export_config` 可以导出 JSON 到临时目录审查
- `apply_config` 才是真正落库

### 最小示例

```json
{
  "action": "plan_config",
  "config": {
    "menus": [
      {
        "name": "User",
        "path": "user",
        "component": "/system/user",
        "title": "用户管理",
        "sort": 20,
        "enabled": true,
        "keep_alive": true,
        "buttons": [
          { "authName": "新增", "authMark": "user:add", "sort": 1, "enabled": true },
          { "authName": "编辑", "authMark": "user:edit", "sort": 2, "enabled": true },
          { "authName": "删除", "authMark": "user:delete", "sort": 3, "enabled": true }
        ],
        "children": []
      }
    ]
  }
}
```

### 重要说明

- 生成器给出的 `menu_config_draft` 是草案，不是最终真值
- `sort` 最好先结合现有菜单树再调整，不要盲信默认值
- `component`、`icon`、`link`、`is_iframe`、`is_full_page` 等字段都可以由 AI 在 apply 前补充或修改
- 如果你要做外链、内嵌、隐藏页、首层页，也应该在 `menu_config_draft` 上改完再交给 `menu_tool`

---

## 八、字典流程

`dict_tool` 也是业务入口，调用时必须带 `action`。

支持的主要动作：

- `list_types`
- `list_data`
- `get_by_type`
- `get_all_enabled`
- `plan_bundle`
- `export_bundle`
- `apply_bundle`
- `create_type`
- `update_type`
- `delete_type`
- `create_data`
- `update_data`
- `delete_data`

### 推荐顺序

1. 读取 `generate_frontend_bundle_from_table` 返回的 `dict_bundle_drafts`
2. 如有必要，补全名称、状态、排序、备注
3. `plan_bundle`
4. 如需文件审查，用 `export_bundle`
5. 确认后再 `apply_bundle`
6. `get_by_type` 验证

### 最小示例

```json
{
  "action": "apply_bundle",
  "operator": "mcp",
  "bundle": {
    "dictName": "用户状态",
    "dictType": "user_status",
    "items": [
      { "dictLabel": "启用", "dictValue": "1", "dictSort": 1 },
      { "dictLabel": "禁用", "dictValue": "2", "dictSort": 2 }
    ]
  }
}
```

### 重要说明

- 如果字段来自 entity 枚举，生成器会自动产出一版 `dict_bundle_drafts`
- 这只是草案，AI 仍然可以在 apply 前修改：
  - `dictName`
  - `status`
  - `remark`
  - `dictSort`
  - `cssClass`
  - `listClass`
  - `isDefault`

---

## 九、Art Design Pro 的特殊约定

如果目标前端是 Art Design Pro，当前推荐做法是：

- `target_preset` 传 `art_design_pro`
- `output_dir` 指向前端项目根目录，不是 `src/views` 子目录

生成结果会落到：

- `src/api`
- `src/types/api`
- `src/views/system/<route-base-kebab>/`

菜单组件路径约定为：

- `"/system/<route-base-kebab>"`

不是：

- `"system/<route-base>/index"`

按钮权限标记约定为：

- `"<route_base>:add"`
- `"<route_base>:edit"`
- `"<route_base>:delete"`

不是裸的：

- `"add"`
- `"edit"`
- `"delete"`

---

## 十、一个完整闭环示例

以 `sys_user` 为例，推荐链路如下：

1. 读取 `schema://tables`
2. 读取 `schema://table/sys_user`
3. 如需样例数据，调用 `table_query`
4. 调用 `generate_entity_from_table`
5. 调用 `generate_admin_module_from_table`
6. 调用 `generate_frontend_bundle_from_table`
7. 读取返回值中的 `menu_config_draft`
8. 读取返回值中的 `dict_bundle_drafts`
9. `menu_tool` 先 `plan_config`
10. `dict_tool` 先 `plan_bundle`
11. 如需导出审查，调用 `export_config` / `export_bundle`
12. 确认后调用 `apply_config` / `apply_bundle`
13. 用 `menu_tool.list_tree` 验证菜单
14. 用 `dict_tool.get_by_type` 验证字典
15. 最后再联调前端页面和后端接口

---

## 十一、Prompt 的定位

当前 server 已发布三个 workflow prompt：

- `discover_table_workflow`
- `generate_crud_bundle_workflow`
- `rollout_menu_dict_workflow`

它们的作用不是替代 tool，而是给 AI 一条稳定的调用顺序提示。

推荐用法：

- 开始读表前，先取 `discover_table_workflow`
- 开始代码生成前，先取 `generate_crud_bundle_workflow`
- 开始落菜单和字典前，先取 `rollout_menu_dict_workflow`

---

## 十二、常见坑

### 1. 忘了给 `menu_tool` / `dict_tool` 传 `action`

这是最常见错误。

这两个 tool 都是：

- `#[serde(tag = "action", rename_all = "snake_case")]`

所以请求体必须带 `action`。

### 2. 以为前端 bundle 会自动落菜单和字典

不会。

`generate_frontend_bundle_from_table` 只负责：

- 生成代码
- 返回菜单草案
- 返回字典草案

真正落库还要继续调用：

- `menu_tool`
- `dict_tool`

### 3. 把复杂业务配置直接写进 SQL

不推荐。

菜单和字典已经有业务入口：

- 可以做 plan
- 可以导出审查
- 可以统一 apply

除非是低层修复，否则不应该退回裸 SQL。

### 4. Art Design Pro 的组件路径写错

如果菜单 component 写成：

- `system/showcase-profile/index`

Art Design Pro 现有路由加载方式下可能找不到组件。

当前约定应使用：

- `/system/showcase-profile`

### 5. 临时 JSON 文件不是 MCP 的必需流程

如果是客户端直接调 MCP tool，请求体直接传 JSON 就可以。

只有在本地 shell 包一层临时 RMCP CLI 客户端时，才可能为了传复杂嵌套参数而先把 JSON 写到临时文件。

这只是调用方式的便利性问题，不是业务流程本身的一部分。

---

## 十三、推荐实践

- 发现结构先走 resource
- 读写数据优先走表工具
- 复杂查询才走 `sql_query_readonly`
- 显式 DDL/DML 才走 `sql_exec`
- 代码先生成到 `/tmp`
- 菜单和字典先 plan/export，再 apply
- apply 后一定回查，不要假设写入成功

---

## 十四、一句话版流程

先读 schema，再用表工具看数据，然后依次生成 entity、后端、前端，接着把前端返回的菜单草案和字典草案交给 `menu_tool` / `dict_tool` 做 plan 和 apply，最后再回查菜单树、字典数据和页面联调结果。
