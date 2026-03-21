# 路由转 SQL 工具使用指南

## 功能说明

这个工具用于将前端 TypeScript 路由配置转换为数据库 SQL 插入语句，实现前后端菜单数据同步。

## 使用步骤

### 1. 导出路由为 JSON

在前端项目中，将路由配置导出为 JSON 格式。

创建一个临时脚本 `export-routes.ts`：

```typescript
import { routeModules } from '@/router/modules'
import fs from 'fs'

// 将路由数据写入 JSON 文件
fs.writeFileSync(
  'routes.json',
  JSON.stringify(routeModules, null, 2)
)

console.log('路由已导出到 routes.json')
```

运行脚本：
```bash
ts-node export-routes.ts
```

### 2. 运行转换工具

```bash
cargo run --bin route_to_sql routes.json 1 > sql/sys/menu_data.sql
```

参数说明：
- `routes.json`: 输入的 JSON 文件路径
- `1`: 起始 ID（可选，默认为 1）
- 输出重定向到 SQL 文件

### 3. 导入数据库

```bash
psql -U postgres -d your_database -f sql/sys/menu_data.sql
```

## JSON 格式示例

```json
[
  {
    "name": "Dashboard",
    "path": "/dashboard",
    "component": "/index/index",
    "redirect": "/dashboard/console",
    "meta": {
      "title": "仪表盘",
      "icon": "ri:pie-chart-line",
      "keepAlive": false,
      "roles": ["R_SUPER", "R_ADMIN"]
    },
    "children": [
      {
        "name": "Console",
        "path": "console",
        "component": "/dashboard/console",
        "meta": {
          "title": "控制台",
          "icon": "ri:home-smile-2-line",
          "keepAlive": false,
          "fixedTab": true
        }
      }
    ]
  }
]
```

## 字段映射

| 前端字段 | 数据库字段 | 说明 |
|---------|-----------|------|
| name | name | 路由名称 |
| path | path | 路由路径 |
| component | component | 组件路径 |
| redirect | redirect | 重定向路径 |
| meta.title | title | 菜单标题 |
| meta.icon | icon | 菜单图标 |
| meta.isHide | is_hide | 是否隐藏 |
| meta.isHideTab | is_hide_tab | 是否隐藏标签页 |
| meta.link | link | 外部链接 |
| meta.isIframe | is_iframe | 是否 iframe |
| meta.keepAlive | keep_alive | 是否缓存 |
| meta.fixedTab | fixed_tab | 是否固定标签页 |
| meta.showBadge | show_badge | 是否显示徽章 |
| meta.showTextBadge | show_text_badge | 文本徽章 |
| meta.activePath | active_path | 激活路径 |
| meta.isFullPage | is_full_page | 是否全屏 |
| meta.isFirstLevel | is_first_level | 是否一级菜单 |
| meta.authList | 子菜单(menu_type=2) | 按钮权限列表 |

## 按钮权限处理

如果路由的 `meta.authList` 不为空，工具会自动为每个权限项创建一条 `menu_type=2` 的记录：

```json
{
  "meta": {
    "authList": [
      { "title": "新增", "authMark": "add" },
      { "title": "编辑", "authMark": "edit" },
      { "title": "删除", "authMark": "delete" }
    ]
  }
}
```

会生成：
```sql
INSERT INTO sys.menu (id, parent_id, menu_type, title, auth_name, auth_mark, sort)
VALUES (10, 5, 2, '新增', '新增', 'add', 1);

INSERT INTO sys.menu (id, parent_id, menu_type, title, auth_name, auth_mark, sort)
VALUES (11, 5, 2, '编辑', '编辑', 'edit', 2);

INSERT INTO sys.menu (id, parent_id, menu_type, title, auth_name, auth_mark, sort)
VALUES (12, 5, 2, '删除', '删除', 'delete', 3);
```

## 注意事项

1. **ID 管理**：工具会自动递增 ID，确保起始 ID 不与现有数据冲突
2. **排序**：子菜单按照在数组中的顺序自动分配 sort 值
3. **转义**：工具会自动处理 SQL 单引号转义
4. **默认值**：未提供的字段使用数据库默认值
5. **国际化**：如果 title 是国际化 key（如 `menus.dashboard.title`），需要手动替换为实际文本

## 完整示例

假设有以下路由配置：

```typescript
export const systemRoutes: AppRouteRecord = {
  path: '/system',
  name: 'System',
  component: '/index/index',
  meta: {
    title: '系统管理',
    icon: 'ri:user-3-line'
  },
  children: [
    {
      path: 'user',
      name: 'User',
      component: '/system/user',
      meta: {
        title: '用户管理',
        icon: 'ri:user-line',
        keepAlive: true,
        authList: [
          { title: '新增', authMark: 'add' },
          { title: '编辑', authMark: 'edit' }
        ]
      }
    }
  ]
}
```

生成的 SQL：

```sql
INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title,
 is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab,
 show_badge, show_text_badge, active_path, sort)
 VALUES (1, 0, 1, 'System', '/system', '/index/index', '', 'ri:user-3-line', '系统管理', false, false, false, false, false, false, false, false, '', '', 0);

INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title,
 is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab,
 show_badge, show_text_badge, active_path, sort)
 VALUES (2, 1, 1, 'User', 'user', '/system/user', '', 'ri:user-line', '用户管理', false, false, false, false, false, true, false, false, '', '', 1);

INSERT INTO sys.menu (id, parent_id, menu_type, title, auth_name, auth_mark, sort)
 VALUES (3, 2, 2, '新增', '新增', 'add', 1);

INSERT INTO sys.menu (id, parent_id, menu_type, title, auth_name, auth_mark, sort)
 VALUES (4, 2, 2, '编辑', '编辑', 'edit', 2);

-- 重置序列
SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu));
```
