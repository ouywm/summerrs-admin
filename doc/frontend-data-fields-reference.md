# 前端数据字段需求参考（供后端数据库设计使用）

> 本文档整理了前端所有页面和组件中涉及的数据字段，供后端进行数据库表设计时参考。
>
> **重要说明：**
> - 本文档**不直接设计数据库表**，只描述前端需要什么字段、什么格式
> - 后端根据实际情况自行设计表结构、字段命名、索引、关联关系等
> - 前端字段名和后端数据库字段名不必完全一致，可以在接口层做映射
> - 标注「前端展示」的字段表示会直接显示在页面上
> - 标注「前端搜索」的字段表示用于搜索/筛选条件

---

## 一、用户模块

用户模块涉及的前端页面较多，数据字段分散在不同场景中，以下按场景逐一梳理。

### 1.1 登录（认证）

**前端页面：** `src/views/auth/login/index.vue`

| 场景 | 字段名 | 类型 | 说明 |
|------|--------|------|------|
| 登录请求 | `userName` | string | 用户名 |
| 登录请求 | `password` | string | 密码 |
| 登录响应 | `token` | string | 访问令牌 |
| 登录响应 | `refreshToken` | string | 刷新令牌 |

**前端行为：**
- 登录成功后前端把 `token` 和 `refreshToken` 存入 localStorage
- 后续所有请求自动在 Header 中携带 `Authorization: {token}`（裸值，无 Bearer 前缀）
- 前端预设了三个测试账号：Super/Admin/User，密码均为 123456

---

### 1.2 注册

**前端页面：** `src/views/auth/register/index.vue`

| 场景 | 字段名 | 类型 | 前端校验 | 说明 |
|------|--------|------|---------|------|
| 注册请求 | `userName` | string | 必填，3-20 字符 | 用户名 |
| 注册请求 | `password` | string | 必填，最少 6 字符 | 密码 |

**前端行为：**
- `confirmPassword`（确认密码）仅在前端校验，不会发给后端
- `agreement`（同意协议）也仅前端使用
- 注册成功后跳转登录页，不自动登录

---

### 1.3 忘记密码

**前端页面：** `src/views/auth/forget-password/index.vue`

| 场景 | 字段名 | 类型 | 说明 |
|------|--------|------|------|
| 请求 | `username` | string | 账号（用户名 / 邮箱 / 手机号，具体方式待定） |

**当前状态：** 页面 UI 已有，但功能为空。后端需要确定密码重置方式（邮箱验证码 / 短信验证码 / 安全问题等），前端再配合补充表单字段。

---

### 1.4 用户信息（登录后获取）

**前端源码：** `src/types/api/api.d.ts` → `Api.Auth.UserInfo`
**使用位置：** 路由守卫、顶栏头像、个人信息展示

| 字段名 | 类型 | 必填 | 用途 |
|--------|------|:----:|------|
| `userId` | number | 是 | 用户唯一标识，用于多账号切换时判断是否同一用户 |
| `userName` | string | 是 | 用户名，前端展示 |
| `email` | string | 是 | 邮箱，前端展示 |
| `avatar` | string | 否 | 头像 URL，为空时前端使用默认头像 |
| `roles` | string[] | 是 | 角色编码列表，如 `["R_ADMIN"]`，用于路由权限和菜单过滤 |
| `buttons` | string[] | 是 | 按钮权限标识列表，如 `["btn_add", "btn_edit"]`，用于按钮级权限控制 |

**后端需要关注：**
- `roles` 是角色**编码**（roleCode），不是角色名称
- `buttons` 是权限**标识**（authMark），前端通过 `v-auth` 指令判断是否显示操作按钮
- 这两个字段决定了用户能看到哪些菜单和能执行哪些操作

---

### 1.5 用户列表（管理页面）

**前端页面：** `src/views/system/user/index.vue`
**前端类型：** `Api.SystemManage.UserListItem`

#### 列表展示字段

| 字段名 | 类型 | 前端展示 | 说明 |
|--------|------|:--------:|------|
| `id` | number | 否（内部用） | 用户 ID |
| `avatar` | string | 是（头像图片） | 头像 URL |
| `userName` | string | 是 | 用户名 |
| `userEmail` | string | 是（在用户名下方） | 邮箱 |
| `userGender` | string | 是（支持排序） | 性别 |
| `userPhone` | string | 是 | 手机号 |
| `status` | string | 是（标签样式） | 状态码（见下方状态值说明） |
| `nickName` | string | 类型中有，页面暂未展示 | 昵称 |
| `userRoles` | string[] | 类型中有，页面暂未展示 | 用户角色编码列表 |
| `createBy` | string | 类型中有，页面暂未展示 | 创建人 |
| `createTime` | string | 是（支持排序） | 创建时间 |
| `updateBy` | string | 类型中有，页面暂未展示 | 更新人 |
| `updateTime` | string | 类型中有，页面暂未展示 | 更新时间 |

#### 用户状态值

前端对 `status` 字段的显示映射（`src/views/system/user/index.vue` 第 76-81 行）：

| status 值 | 显示文本 | 标签颜色 |
|:---------:|---------|---------|
| `"1"` | 在线 | success（绿色） |
| `"2"` | 离线 | info（灰色） |
| `"3"` | 异常 | warning（橙色） |
| `"4"` | 注销 | danger（红色） |

> 注意：前端 `status` 字段是 **string 类型**，不是 number。

#### 搜索条件字段

**前端组件：** `src/views/system/user/modules/user-search.vue`

| 字段名 | 搜索方式 | 说明 |
|--------|---------|------|
| `userName` | 文本输入 | 用户名模糊搜索 |
| `userPhone` | 文本输入（最大 11 位） | 手机号 |
| `userEmail` | 文本输入 | 邮箱 |
| `status` | 下拉选择（"1"/"2"/"3"/"4"） | 状态筛选 |
| `userGender` | 单选按钮（"1"=男 / "2"=女） | 性别筛选 |

> 注意：搜索中性别值是 `"1"` 和 `"2"`，但列表展示中 `userGender` 直接显示中文 "男"/"女"。后端需要统一，建议列表中也返回编码，前端做映射；或者搜索和展示使用同一种格式。

#### 新增/编辑用户字段

**前端组件：** `src/views/system/user/modules/user-dialog.vue`

| 字段名 | 类型 | 必填 | 前端校验规则 | 说明 |
|--------|------|:----:|-------------|------|
| `username` | string | 是 | 2-20 字符 | 用户名 |
| `phone` | string | 是 | 正则 `/^1[3-9]\d{9}$/` | 手机号 |
| `gender` | string | 是 | 默认值 "男" | 性别（"男" / "女"） |
| `role` | string[] | 是 | 至少选一个 | 角色编码列表 |

**编辑模式回填映射：**
```
表单字段 username ← 列表字段 userName
表单字段 phone    ← 列表字段 userPhone
表单字段 gender   ← 列表字段 userGender
表单字段 role     ← 列表字段 userRoles
```

> 编辑时前端用 `id` 标识用户，通过 URL 路径传递。

---

### 1.6 用户模块字段汇总

将上述所有场景中涉及的用户字段合并，后端设计用户相关表时参考：

| 字段 | 出现场景 | 必须存储 | 说明 |
|------|---------|:--------:|------|
| 用户 ID | 用户信息、用户列表 | 是 | 主键 |
| 用户名 | 登录、注册、用户信息、用户列表、新增编辑 | 是 | 唯一 |
| 密码 | 登录、注册 | 是 | 加密存储 |
| 昵称 | 用户列表 | 否 | 类型中定义，页面暂未展示 |
| 邮箱 | 用户信息、用户列表 | 是 | |
| 手机号 | 用户列表、新增编辑 | 是 | |
| 性别 | 用户列表、新增编辑、搜索 | 是 | |
| 头像 URL | 用户信息、用户列表 | 否 | |
| 状态 | 用户列表、搜索 | 是 | "1"在线/"2"离线/"3"异常/"4"注销 |
| 角色列表 | 用户信息(roles)、用户列表(userRoles)、新增编辑(role) | 是 | 多对多关系 |
| 按钮权限 | 用户信息(buttons) | 是 | 可通过角色关联推导 |
| 创建人 | 用户列表 | 否 | |
| 创建时间 | 用户列表 | 是 | 格式 YYYY-MM-DD HH:mm:ss |
| 更新人 | 用户列表 | 否 | |
| 更新时间 | 用户列表 | 否 | |
| Token | 登录响应 | — | 可以不落库（JWT 无状态） |
| RefreshToken | 登录响应 | — | 建议落库或 Redis 管理 |

---

## 二、角色模块

### 2.1 角色列表

**前端页面：** `src/views/system/role/index.vue`
**前端类型：** `Api.SystemManage.RoleListItem`

#### 列表展示字段

| 字段名 | 类型 | 前端展示 | 说明 |
|--------|------|:--------:|------|
| `roleId` | number | 是 | 角色 ID |
| `roleName` | string | 是 | 角色名称 |
| `roleCode` | string | 是 | 角色编码（如 R_ADMIN） |
| `description` | string | 是（超长省略提示） | 角色描述 |
| `enabled` | boolean | 是（标签样式） | 启用状态（true=启用/绿色, false=禁用/橙色） |
| `createTime` | string | 是（支持排序） | 创建日期 |

#### 搜索条件字段

**前端组件：** `src/views/system/role/modules/role-search.vue`

| 字段名 | 搜索方式 | 说明 |
|--------|---------|------|
| `roleName` | 文本输入 | 角色名称 |
| `roleCode` | 文本输入 | 角色编码 |
| `description` | 文本输入 | 角色描述 |
| `enabled` | 下拉选择（true/false） | 角色状态 |
| `startTime` | 日期范围（YYYY-MM-DD） | 创建日期起始 |
| `endTime` | 日期范围（YYYY-MM-DD） | 创建日期截止 |

> 前端搜索栏使用 `daterange` 组件，提交时会转换为 `startTime` 和 `endTime` 两个参数。

#### 新增/编辑角色字段

**前端组件：** `src/views/system/role/modules/role-edit-dialog.vue`

| 字段名 | 类型 | 必填 | 前端校验规则 | 说明 |
|--------|------|:----:|-------------|------|
| `roleName` | string | 是 | 2-20 字符 | 角色名称 |
| `roleCode` | string | 是 | 2-50 字符 | 角色编码 |
| `description` | string | 是 | — | 角色描述（多行文本） |
| `enabled` | boolean | 否 | 默认 true | 是否启用（Switch 开关） |

---

### 2.2 角色权限分配

**前端组件：** `src/views/system/role/modules/role-permission-dialog.vue`

前端使用 Element Plus 的 `ElTree`（树形选择）组件来分配菜单权限。

#### 前端需要的数据

**读取权限时（GET）：**

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `checkedKeys` | string[] | 完全选中的菜单 `name` 列表 |
| `halfCheckedKeys` | string[] | 半选状态的父级菜单 `name` 列表 |

> `name` 对应菜单路由的 `name` 字段，如 `"DashboardConsole"`、`"SystemUser"` 等。
> 半选状态：父级菜单下的子菜单只有部分被选中时，父级为半选。
> **必须同时存储 checkedKeys 和 halfCheckedKeys**，否则回显时树形控件的选中状态会不正确。

**保存权限时（PUT）：** 同上，前端会把 `checkedKeys` 和 `halfCheckedKeys` 都提交。

### 2.3 角色模块字段汇总

| 字段 | 出现场景 | 必须存储 | 说明 |
|------|---------|:--------:|------|
| 角色 ID | 列表、编辑、权限 | 是 | 主键 |
| 角色名称 | 列表、新增编辑、搜索 | 是 | |
| 角色编码 | 列表、新增编辑、搜索、用户角色关联 | 是 | 唯一，前端各处引用此值 |
| 角色描述 | 列表、新增编辑、搜索 | 是 | |
| 是否启用 | 列表、新增编辑、搜索 | 是 | boolean |
| 创建时间 | 列表、搜索 | 是 | |
| 菜单权限 | 权限分配 | 是 | 多对多关系（角色 ↔ 菜单 name） |

---

## 三、菜单模块

### 3.1 菜单列表（GET 接口返回的树形结构）

**前端类型：** `src/types/router/index.ts` → `AppRouteRecord`

菜单接口返回的是**嵌套树形数据**，不是平铺列表。每个节点的字段如下：

| 字段名 | 类型 | 层级 | 说明 |
|--------|------|------|------|
| `path` | string | 所有 | 路由路径。一级以 `/` 开头，子级不以 `/` 开头 |
| `name` | string | 所有 | 路由名称（**唯一标识**），角色权限分配引用此值 |
| `component` | string | 叶子节点 | 组件路径（如 `"dashboard/console/index"`） |
| `redirect` | string | 有子菜单时 | 重定向路径 |
| `children` | array | 有子菜单时 | 子菜单列表（递归） |
| `meta.title` | string | 所有 | **菜单标题（必须）** |
| `meta.icon` | string | 一般一级菜单 | 菜单图标 |
| `meta.isHide` | boolean | 可选 | 是否在菜单中隐藏 |
| `meta.isHideTab` | boolean | 可选 | 是否在标签页中隐藏 |
| `meta.link` | string | 可选 | 外部链接 URL |
| `meta.isIframe` | boolean | 可选 | 是否为 iframe 内嵌 |
| `meta.keepAlive` | boolean | 可选 | 是否缓存页面 |
| `meta.roles` | string[] | 可选 | 允许访问的角色编码列表 |
| `meta.isFirstLevel` | boolean | 可选 | 是否为一级菜单（无子菜单的独立页面） |
| `meta.fixedTab` | boolean | 可选 | 是否固定标签页 |
| `meta.activePath` | string | 可选 | 高亮的菜单路径（用于详情页等隐藏菜单） |
| `meta.isFullPage` | boolean | 可选 | 是否全屏页面 |
| `meta.showBadge` | boolean | 可选 | 是否显示小圆点徽章 |
| `meta.showTextBadge` | string | 可选 | 文本徽章内容（如 "New"） |
| `meta.authList` | array | 可选 | 操作权限列表（见下方） |

#### meta.authList 结构

每个菜单可以关联多个操作权限（按钮级权限）：

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `title` | string | 权限名称（如 "新增"、"编辑"、"删除"） |
| `authMark` | string | 权限标识（如 "add"、"edit"、"delete"） |

> 前端通过 `v-auth="'add'"` 指令判断当前用户是否有此权限标识。

---

### 3.2 菜单新增/编辑（表单字段）

**前端组件：** `src/views/system/menu/modules/menu-dialog.vue`

菜单分为两种类型：**菜单**（menu）和**按钮权限**（button）。

#### 类型一：菜单（menuType = 'menu'）

| 字段名 | 类型 | 必填 | 说明 |
|--------|------|:----:|------|
| `name` | string | 是 | 菜单名称（2-20 字符） |
| `path` | string | 是 | 路由地址（一级以 `/` 开头，子级不以 `/` 开头） |
| `label` | string | 是 | 权限标识 / 路由 name |
| `component` | string | 否 | 组件路径（如 `/system/user`，目录菜单留空） |
| `icon` | string | 否 | 图标标识（如 `ri:user-line`） |
| `sort` | number | 否 | 排序号（默认 1） |
| `roles` | string[] | 否 | 角色权限（输入角色编码，回车添加） |
| `link` | string | 否 | 外部链接 URL |
| `showTextBadge` | string | 否 | 文本徽章（如 "New"、"Hot"） |
| `activePath` | string | 否 | 激活路径（隐藏菜单高亮用） |
| `isEnable` | boolean | 否 | 是否启用（默认 true） |
| `keepAlive` | boolean | 否 | 页面缓存（默认 true） |
| `isHide` | boolean | 否 | 隐藏菜单（默认 false） |
| `isIframe` | boolean | 否 | 是否内嵌（默认 false） |
| `showBadge` | boolean | 否 | 显示徽章（默认 false） |
| `fixedTab` | boolean | 否 | 固定标签（默认 false） |
| `isHideTab` | boolean | 否 | 标签隐藏（默认 false） |
| `isFullPage` | boolean | 否 | 全屏页面（默认 false） |

#### 类型二：按钮权限（menuType = 'button'）

| 字段名 | 类型 | 必填 | 说明 |
|--------|------|:----:|------|
| `authName` | string | 是 | 权限名称（如 "新增"、"编辑"） |
| `authLabel` | string | 是 | 权限标识（如 "add"、"edit"） |
| `authSort` | number | 否 | 权限排序（默认 1） |

> 按钮权限属于某个菜单的子项，对应菜单列表中 `meta.authList` 的每一项。

---

### 3.3 菜单模块字段汇总

| 字段 | 出现场景 | 必须存储 | 说明 |
|------|---------|:--------:|------|
| 菜单 ID | 编辑、删除 | 是 | 主键 |
| 父级 ID | 树形结构 | 是 | 用于构建树形，0 表示一级 |
| 菜单类型 | 新增编辑 | 是 | menu（菜单）/ button（按钮权限） |
| 菜单名称（title） | 列表、新增编辑 | 是 | |
| 路由路径（path） | 列表、新增编辑 | 菜单必填 | |
| 路由名称（name/label） | 列表、新增编辑、权限引用 | 菜单必填 | 全局唯一 |
| 组件路径（component） | 菜单列表 | 否 | 叶子菜单填写 |
| 重定向（redirect） | 菜单列表 | 否 | 有子菜单时 |
| 图标（icon） | 列表、新增编辑 | 否 | |
| 排序（sort） | 新增编辑 | 否 | |
| 是否启用 | 列表、新增编辑 | 是 | |
| 各开关字段 | 新增编辑 | 否 | keepAlive, isHide, isHideTab, isIframe 等 |
| 外部链接（link） | 新增编辑 | 否 | |
| 角色权限（roles） | 列表 meta | 否 | 用于前端权限模式 |
| 权限名称（authName） | 按钮权限 | 按钮必填 | |
| 权限标识（authMark） | 按钮权限 | 按钮必填 | |

---

## 四、文章模块

### 4.1 文章列表

**前端页面：** `src/views/article/list/index.vue`
**Mock 数据：** `src/mock/temp/articleList.ts`

#### 列表展示字段

| 字段名 | 类型 | 前端展示 | 说明 |
|--------|------|:--------:|------|
| `id` | number | 否（内部用） | 文章 ID |
| `title` | string | 是 | 文章标题 |
| `home_img` | string | 是（封面图） | 封面图片 URL |
| `type_name` | string | 是（右上角标签） | 分类名称 |
| `blog_class` | string | 否（内部用） | 分类 ID |
| `create_time` | string | 是（格式化为 YYYY-MM-DD） | 创建时间 |
| `count` | number | 是（阅读量） | 浏览次数 |

> Mock 数据中还有 `brief`（摘要）和 `html_content` 字段，但列表页面未使用。

#### 搜索条件字段

| 字段名 | 搜索方式 | 说明 |
|--------|---------|------|
| `searchVal` | 文本输入（回车搜索） | 标题关键词 |
| `year` | 分段选择器（"All"/"2024"/"2023"/...） | 按年份筛选 |
| `current` | 分页 | 页码（默认 1） |
| `size` | 分页 | 每页条数（默认 40） |

---

### 4.2 文章详情

**前端页面：** `src/views/article/detail/index.vue`

| 字段名 | 类型 | 前端展示 | 说明 |
|--------|------|:--------:|------|
| `title` | string | 是（页面标题） | 文章标题 |
| `html_content` | string | 是（富文本渲染） | 文章 HTML 内容 |

> 详情页通过 `v-html` 渲染富文本，所以后端存储的是 HTML 格式的内容。

---

### 4.3 文章发布/编辑

**前端页面：** `src/views/article/publish/index.vue`

#### 发布表单字段

| 字段名 | 类型 | 必填 | 说明 |
|--------|------|:----:|------|
| `title` | string | 是 | 文章标题（最多 100 字符） |
| `type` | number | 是 | 分类 ID（从分类列表下拉选择） |
| `content` | string | 是 | 文章 HTML 内容（wangEditor 富文本编辑器输出） |
| `cover` | string | 是 | 封面图 URL（通过上传接口获得） |
| `visible` | boolean | 否 | 是否可见（Switch 开关，默认 true） |

#### 编辑模式回填

编辑时通过 URL query 参数 `id` 获取文章详情，回填到表单：

| 表单字段 | ← 详情字段 |
|---------|-----------|
| `title`（articleName） | ← `title` |
| `type`（articleType） | ← `blog_class`（Number 转换） |
| `content`（editorHtml） | ← `html_content` |

> 编辑模式下还需要回填 `cover`（封面图）和 `visible`（可见性），当前详情接口未返回这两个字段，后端应补充。

---

### 4.4 文章分类

**前端页面：** `src/views/article/publish/index.vue`（用于发布时选择分类）

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `id` | number | 分类 ID（作为 ElOption 的 value） |
| `name` | string | 分类名称（作为 ElOption 的 label） |

> 返回全量列表，不分页。用于下拉选择框。

---

### 4.5 文章模块字段汇总

| 字段 | 出现场景 | 必须存储 | 说明 |
|------|---------|:--------:|------|
| 文章 ID | 列表、详情、编辑 | 是 | 主键 |
| 标题 | 列表、详情、发布编辑 | 是 | 最大 100 字符 |
| 分类 ID | 列表、发布编辑 | 是 | 外键关联分类表 |
| 分类名称 | 列表展示 | — | 可通过 JOIN 获取，或冗余存储 |
| HTML 内容 | 详情、发布编辑 | 是 | 富文本内容，可能较大 |
| 摘要 | Mock 中有 | 否 | 可由后端从内容中截取 |
| 封面图 URL | 列表、发布编辑 | 是 | |
| 是否可见 | 发布编辑 | 是 | |
| 浏览次数 | 列表展示 | 是 | 默认 0 |
| 创建时间 | 列表展示 | 是 | |
| 分类 ID（分类表） | 分类列表 | 是 | 主键 |
| 分类名称（分类表） | 分类列表 | 是 | |

---

## 五、留言/评论模块

前端有两个不同的评论相关场景，数据结构不同。

### 5.1 留言墙（扁平列表）

**前端页面：** `src/views/article/comment/index.vue`
**Mock 数据：** `src/mock/temp/commentList.ts`

| 字段名 | 类型 | 前端展示 | 说明 |
|--------|------|:--------:|------|
| `id` | number | 否 | 留言 ID |
| `date` | string | 是 | 留言日期（如 "2024-9-3"） |
| `content` | string | 是 | 留言内容 |
| `collection` | number | 是（心形图标 + 数量） | 收藏/点赞数 |
| `comment` | number | 是（消息图标 + 数量） | 评论数 |
| `userName` | string | 是 | 留言人昵称 |

> 留言墙是**扁平列表**，不是树形结构。每条留言显示为一张卡片。

---

### 5.2 评论组件（树形嵌套）

**前端组件：** `src/components/business/comment-widget/index.vue`
**Mock 数据：** `src/mock/temp/commentDetail.ts`

这是一个通用的评论组件，支持**无限层级嵌套回复**。

| 字段名 | 类型 | 前端展示 | 说明 |
|--------|------|:--------:|------|
| `id` | number | 否 | 评论 ID |
| `author` | string | 是 | 评论者名称 |
| `content` | string | 是 | 评论内容 |
| `timestamp` | string | 是 | 评论时间（如 "2024-09-04 09:00"） |
| `replies` | Comment[] | 是（嵌套展示） | 回复列表（递归结构） |

#### 发表评论字段

| 字段名 | 类型 | 必填 | 说明 |
|--------|------|:----:|------|
| `author` | string | 是 | 评论者名称 |
| `content` | string | 是 | 评论内容 |

#### 回复评论字段

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `commentId` | number | 被回复的评论 ID |
| `author` | string | 回复者名称 |
| `content` | string | 回复内容 |

### 5.3 留言/评论模块字段汇总

| 字段 | 出现场景 | 必须存储 | 说明 |
|------|---------|:--------:|------|
| 留言 ID | 留言墙 | 是 | 主键 |
| 留言内容 | 留言墙 | 是 | |
| 留言日期 | 留言墙 | 是 | |
| 留言人名称 | 留言墙 | 是 | 可以是昵称或"匿名" |
| 点赞数 | 留言墙 | 是 | |
| 评论数 | 留言墙 | 是 | 可通过统计子评论得出 |
| 评论 ID | 评论组件 | 是 | 主键 |
| 父评论 ID | 评论组件 | 是 | 用于构建树形，0 表示顶级评论 |
| 关联留言 ID | 评论组件 | 是 | 此评论属于哪条留言 |
| 评论者名称 | 评论组件 | 是 | |
| 评论内容 | 评论组件 | 是 | |
| 评论时间 | 评论组件 | 是 | |

---

## 六、文件上传

**前端页面：** `src/views/article/publish/index.vue`

| 项目 | 说明 |
|------|------|
| 上传地址 | `POST /api/common/upload` |
| 请求格式 | `multipart/form-data` |
| 字段名 | `file` |
| 文件限制 | 仅图片（image/*），最大 2MB |

**上传成功响应需要的字段：**

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `url` | string | 文件的完整可访问 URL |

> 后端需要提供文件存储服务（本地磁盘 / OSS / CDN），返回的 URL 前端直接作为图片 `src` 使用。

---

## 七、分页规范

所有分页接口，前端使用统一的分页参数和响应格式。

**请求参数：**

| 字段名 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `current` | number | 1 | 页码，**从 1 开始** |
| `size` | number | 10 或 20 | 每页条数 |

**响应结构：**

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `records` | array | 当前页数据列表 |
| `current` | number | 当前页码 |
| `size` | number | 每页条数 |
| `total` | number | **总记录数（必须准确，用于计算总页数）** |

> 各页面默认 `size`：用户管理 20、角色管理 20、文章列表 40。

---

## 八、数据格式约定

| 类型 | 格式 | 示例 |
|------|------|------|
| 时间 | `YYYY-MM-DD HH:mm:ss` | `2024-06-15 10:30:00` |
| 日期 | `YYYY-MM-DD` | `2024-06-15` |
| 布尔值 | `true` / `false`（JSON boolean） | 角色 enabled 字段 |
| 状态码字符串 | `"1"` / `"2"` / `"3"` / `"4"` | 用户 status 字段（注意是 string） |
| URL | 完整可访问的 HTTP(S) 地址 | 头像、封面图 |
| 数组 | JSON Array | 角色列表 `["R_ADMIN", "R_USER"]` |

---

*文档版本：v1.0*
*更新日期：2026-02-26*
*适用前端版本：art-design-pro v3.0.1*