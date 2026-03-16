# sys_config 生成流程复盘

这份文档记录本次 `sys_config` 模块从表设计到代码生成过程中暴露出来的问题，重点不是追责，而是把后续同类模块的流程收紧。

---

## 一、这次流程里暴露出的不足

### 1. 生成器缺少显式菜单父级参数

最初 `generate_frontend_bundle_from_table` 只能返回顶级菜单草案。

这在简单项目里还能接受，但当前系统菜单明确挂在 `/system` 下，所以生成结果天然不完整，后续必须人工再包一层父节点。

问题本质：

- 生成器没有拿到足够的菜单上下文
- AI 只能在生成后补救，而不是在生成时一次成型

本次已补上：

- `menu_parent`

---

### 2. 前端 UI 推断过度依赖数据库注释

这次预览里出现过两个明显错误：

- `value_type` 被误判成密码输入
- `option_dict_type` 被误推断成错误字典类型

问题本质：

- 数据库注释是后端语义说明，不应该直接等价为前端渲染配置
- 生成器需要把“字段事实”和“UI 决策”分开

当前改进方向：

- 保留注释用于基础语义
- 让 AI 通过 `field_ui_meta`、`field_hints`、`dict_bindings` 显式给参数
- 生成器只做受控推断，不做激进猜测

---

### 3. 预览产物和真实落地产物曾经不一致

前面一轮虽然已经生成出了前端代码，但实际落在的是仓库内的 `crates/app/frontend-routes`，不是真实前端项目。

这会形成一种假完成状态：

- 代码“看起来已经有了”
- 但真实运行项目并没有新增页面

后续约束应该更明确：

- 如果目标是 `art_design_pro`
- `output_dir` 就直接指向真实前端项目根目录
- 不再先落临时目录再人工搬运

---

### 4. MCP 调用链路对“运行的是哪一个生成器”不够透明

这次还有一个实际问题：会话内可用的 MCP 工具，不一定就是你刚改完源码后的那份 server 进程。

结果就是：

- 代码已经改了
- 但调用时仍可能命中旧进程
- 于是出现“我明明修了，为什么生成结果还是旧逻辑”

当前更稳的做法：

- 需要验证最新生成逻辑时，优先走本地 `summerrs-mcp` 二进制
- 明确指定请求 JSON、数据库 URL、输出目录

这件事后续最好制度化，而不是靠临场经验。

---

### 5. 缺少一份生成前检查清单

这次很多问题并不是代码写错，而是参数不完整。

尤其在前端 bundle 场景，至少要先确定：

- `target_preset`
- `output_dir`
- `route_base`
- `search_fields`
- `table_fields`
- `form_fields`
- `field_ui_meta`
- `dict_bindings`
- `menu_parent`

如果这些参数没有在生成前确认，生成器再聪明也只能猜。

---

## 二、后续建议固定成的流程

推荐把“先查清楚，再生成，再落库，再联调”变成固定顺序：

1. 确认表结构和字段注释只表达数据语义，不夹带前端实现细节。
2. `menu_tool.list_tree` 获取真实菜单树，提取目标父节点。
3. 明确 `target_preset` 和真实 `output_dir`。
4. 按业务字段补齐 `field_ui_meta`、`dict_bindings`、显式字段列表。
5. 调用 `generate_entity_from_table`。
6. 调用 `generate_admin_module_from_table`。
7. 调用 `generate_frontend_bundle_from_table`，并显式传 `menu_parent`。
8. 审查返回的 `menu_config_draft` 和 `dict_bundle_drafts`。
9. 分别用 `menu_tool.plan_config`、`dict_tool.plan_bundle` 做预演。
10. 最后再决定是否 `apply`。

---

## 三、还值得继续补的能力

下面这些能力如果补上，后续同类模块的生成会更稳：

- 为 `generate_frontend_bundle_from_table` 增加更清晰的 preflight 提示，缺关键参数时直接提示风险。
- 把“真实前端项目输出路径检查”前置，避免生成到错误目录。
- 增加一条面向真实项目的 smoke check，例如校验目标页面、API 文件、类型文件是否都已存在。
- 在文档里给出 `menu_parent`、`field_ui_meta`、`dict_bindings` 的标准样例，减少 AI 即兴发挥空间。

---

## 四、结论

这次流程的主要问题，不是 CRUD 生成能力不够，而是“生成前上下文没有被明确参数化”。

一旦把这些上下文改成显式参数，很多原本需要人工补救的问题都可以在生成阶段一次解决。
