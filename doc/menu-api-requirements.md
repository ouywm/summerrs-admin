# 菜单管理 API 接口要求

## 问题说明

当前前端菜单管理页面（`/system/menu`）使用的数据格式与后端返回的格式不兼容，需要后端调整接口返回格式。

## 接口对比

### 1. 当前用户菜单接口（正常）✅

**接口**: `GET /api/v3/system/menus`
**用途**: 获取当前用户的菜单树（用于渲染左侧菜单和动态路由）
**返回**: `MenuTreeVo[]` - 树形结构
**状态**: ✅ 正常，无需修改

### 2. 菜单管理接口（需要修改）❌

**接口**: `GET /api/system/menu/list`
**用途**: 菜单管理页面的 CRUD 操作
**当前返回**: `MenuVo[]` - 扁平结构
**问题**: 前端期望树形结构，当前返回扁平结构

---

## 解决方案（二选一）

### 方案 A：后端返回树形结构（推荐）✅

**修改接口**: `GET /api/system/menu/list`
**返回类型**: 改为 `MenuTreeVo[]`（树形结构）

**优点**:
- 前端无需转换，直接使用
- 性能更好
- 代码更简洁

**缺点**:
- 后端需要做树形转换

### 方案 B：前端转换（不推荐）❌

**保持接口**: `GET /api/system/menu/list` 返回 `MenuVo[]`
**前端处理**: 将扁平结构转换为树形结构

**优点**:
- 后端无需修改

**缺点**:
- 前端需要写转换逻辑
- 性能较差
- 代码复杂

---

## 推荐方案：后端返回树形结构

### 接口定义

```rust
/// 获取所有菜单列表（管理用）- 返回树形结构
#[get("/system/menu/list")]
pub async fn list_menus(
    Component(svc): Component<SysMenuService>,
    Query(query): Query<MenuQueryDto>,
) -> ApiResult<ApiResponse<Vec<MenuTreeVo>>> {  // 改为 MenuTreeVo
    let vo = svc.list_menus_tree(query).await?;  // 返回树形结构
    Ok(ApiResponse::ok(vo))
}
```

### 返回数据格式

```json
[
  {
    "path": "/system",
    "name": "System",
    "component": "/index/index",
    "redirect": "/system/user",
    "meta": {
      "title": "系统管理",
      "icon": "ri:user-3-line",
      "isHide": false,
      "isHideTab": false,
      "link": "",
      "isIframe": false,
      "keepAlive": true,
      "roles": ["R_SUPER", "R_ADMIN"],
      "isFirstLevel": false,
      "fixedTab": false,
      "activePath": "",
      "isFullPage": false,
      "showBadge": false,
      "showTextBadge": "",
      "authList": []
    },
    "children": [
      {
        "path": "user",
        "name": "SystemUser",
        "component": "/system/user",
        "redirect": "",
        "meta": {
          "title": "用户管理",
          "icon": "ri:user-line",
          "isHide": false,
          "isHideTab": false,
          "link": "",
          "isIframe": false,
          "keepAlive": true,
          "roles": [],
          "isFirstLevel": false,
          "fixedTab": false,
          "activePath": "",
          "isFullPage": false,
          "showBadge": false,
          "showTextBadge": "",
          "authList": [
            {
              "title": "新增",
              "authMark": "add"
            },
            {
              "title": "编辑",
              "authMark": "edit"
            },
            {
              "title": "删除",
              "authMark": "delete"
            }
          ]
        },
        "children": []
      }
    ]
  }
]
```

---

## 字段映射关系

### 数据库字段 → MenuTreeVo 字段

| 数据库字段 | MenuTreeVo 字段 | 说明 |
|-----------|----------------|------|
| `name` | `name` | 路由名称 |
| `path` | `path` | 路由路径 |
| `component` | `component` | 组件路径 |
| `redirect` | `redirect` | 重定向路径 |
| `title` | `meta.title` | 菜单标题 |
| `icon` | `meta.icon` | 菜单图标 |
| `is_hide` | `meta.isHide` | 是否隐藏 |
| `is_hide_tab` | `meta.isHideTab` | 标签页隐藏 |
| `link` | `meta.link` | 外部链接 |
| `is_iframe` | `meta.isIframe` | 是否内嵌 |
| `keep_alive` | `meta.keepAlive` | 页面缓存 |
| `is_first_level` | `meta.isFirstLevel` | 一级菜单 |
| `fixed_tab` | `meta.fixedTab` | 固定标签 |
| `active_path` | `meta.activePath` | 激活路径 |
| `is_full_page` | `meta.isFullPage` | 全屏页面 |
| `show_badge` | `meta.showBadge` | 显示徽章 |
| `show_text_badge` | `meta.showTextBadge` | 文本徽章 |
| `parent_id` | - | 用于构建树形结构 |
| `menu_type=2` 的记录 | `meta.authList[]` | 按钮权限转为 authList |

### 按钮权限处理

**数据库中 `menu_type = 2` 的记录**（按钮权限）应该：
1. **不作为独立节点**
2. **合并到父菜单的 `meta.authList` 数组中**

示例：
```sql
-- 父菜单
id=5, parent_id=4, menu_type=1, name='SystemUser', title='用户管理'

-- 按钮权限（这些不应该作为 children）
id=8,  parent_id=5, menu_type=2, title='新增', auth_mark='add'
id=9,  parent_id=5, menu_type=2, title='编辑', auth_mark='edit'
id=10, parent_id=5, menu_type=2, title='删除', auth_mark='delete'
```

**转换后**：
```json
{
  "name": "SystemUser",
  "path": "user",
  "meta": {
    "title": "用户管理",
    "authList": [
      { "title": "新增", "authMark": "add" },
      { "title": "编辑", "authMark": "edit" },
      { "title": "删除", "authMark": "delete" }
    ]
  },
  "children": []
}
```

---

## 树形结构构建逻辑

### 伪代码

```rust
fn build_menu_tree(menus: Vec<MenuModel>) -> Vec<MenuTreeVo> {
    // 1. 分离菜单和按钮权限
    let menu_items: Vec<_> = menus.iter().filter(|m| m.menu_type == 1).collect();
    let auth_items: Vec<_> = menus.iter().filter(|m| m.menu_type == 2).collect();

    // 2. 构建按钮权限映射 parent_id -> Vec<AuthItem>
    let auth_map: HashMap<i64, Vec<AuthItem>> = auth_items
        .into_iter()
        .fold(HashMap::new(), |mut map, auth| {
            map.entry(auth.parent_id)
                .or_insert_with(Vec::new)
                .push(AuthItem {
                    title: auth.title.clone(),
                    auth_mark: auth.auth_mark.clone(),
                });
            map
        });

    // 3. 递归构建树形结构
    fn build_tree(
        parent_id: i64,
        all_menus: &[&MenuModel],
        auth_map: &HashMap<i64, Vec<AuthItem>>
    ) -> Vec<MenuTreeVo> {
        all_menus
            .iter()
            .filter(|m| m.parent_id == parent_id)
            .map(|menu| {
                let children = build_tree(menu.id, all_menus, auth_map);
                let auth_list = auth_map.get(&menu.id).cloned().unwrap_or_default();

                MenuTreeVo {
                    path: menu.path.clone(),
                    name: menu.name.clone(),
                    component: menu.component.clone(),
                    redirect: menu.redirect.clone(),
                    meta: MenuMeta {
                        title: menu.title.clone(),
                        icon: menu.icon.clone(),
                        // ... 其他字段
                        auth_list,
                    },
                    children,
                }
            })
            .collect()
    }

    // 4. 从根节点（parent_id = 0）开始构建
    build_tree(0, &menu_items, &auth_map)
}
```

---

## 查询参数支持

`MenuQueryDto` 应该支持以下查询条件：

```rust
pub struct MenuQueryDto {
    pub name: Option<String>,      // 路由名称模糊查询
    pub path: Option<String>,      // 路由路径模糊查询
    pub title: Option<String>,     // 菜单标题模糊查询
    pub menu_type: Option<MenuType>, // 菜单类型筛选
    pub enabled: Option<bool>,     // 启用状态筛选
}
```

**注意**: 查询时仍然返回树形结构，但只包含匹配的节点及其父节点路径。

---

## 总结

### 后端需要修改的地方

1. ✅ 修改 `GET /api/system/menu/list` 接口
2. ✅ 返回类型改为 `Vec<MenuTreeVo>`
3. ✅ 实现树形结构构建逻辑
4. ✅ 将 `menu_type=2` 的按钮权限合并到父菜单的 `authList`
5. ✅ 支持查询参数过滤

### 前端无需修改

前端代码已经按照 `MenuTreeVo` 格式编写，后端返回正确格式后即可直接使用。
