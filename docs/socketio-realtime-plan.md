# Socket.IO 实时能力规划

这份文档用于沉淀 `summerrs-admin` 的实时通信规划，先明确方向、边界和分阶段落地方案，后续按节奏逐步实现，不一次做重。

---

## 一、结论

当前项目实时能力优先选择 `Socket.IO`，不优先选择裸 `WebSocket`。

原因：

- 这个项目是后台管理模板，实时场景更偏“事件推送”，不是底层协议实验。
- `Socket.IO` 自带命名事件、自动重连、心跳、ack、room、namespace，做后台业务更顺手。
- 当前后端框架已经具备 `socket_io` 集成能力，接入成本明显低于从零封装裸 `WebSocket`。
- 后续要做在线用户、公告通知、强制下线、任务进度，这些都更适合事件模型。

不选择裸 `WebSocket` 的核心原因不是它不能做，而是：

- 需要自己维护消息协议
- 需要自己补重连、心跳、房间、广播
- 第一版开发成本更高
- 代码更容易散落到各业务模块里

---

## 二、当前项目已有基础

项目当前已经具备 Socket.IO 落地基础：

- `crates/app/Cargo.toml`
  - `summer-web` 已启用 `socket_io`
- `summer-web` 已支持：
  - `#[on_connection]`
  - `#[on_disconnect]`
  - `#[subscribe_message("event")]`
- `SocketIo` 可以作为组件注入，在 service / handler 中主动发事件

这意味着：

- 不需要先自己封装一整套底层通信层
- 可以直接基于框架做业务级实时推送

---

## 三、这个项目最适合做的实时场景

第一阶段建议只做后台模板真正需要的能力。

### 1. 在线用户实时刷新

适用场景：

- 新用户登录后，在线用户页面自动刷新
- 用户退出、被踢下线后，页面自动刷新

建议事件：

- `online.changed`
- `session.kickout`

---

### 2. 公告通知

这个模块非常适合走 Socket.IO。

适用场景：

- 管理员发布公告后，在线用户立即收到
- 指定角色用户收到定向通知
- 用户已读后，未读数实时变化

建议事件：

- `notice.created`
- `notice.unread.changed`
- `notice.read`

---

### 3. 简单任务进度推送

适用场景：

- 导入导出
- 大文件处理
- 后台异步任务状态变化

建议事件：

- `job.progress`
- `job.finished`
- `job.failed`

---

### 4. 监控类页面局部实时刷新

适用场景：

- 在线数
- 当前请求量
- 某些轻量状态看板

建议事件：

- `monitor.metrics.changed`

注意：

- 第一版不要做成高频流式监控
- 只做轻量、低频、事件型刷新

---

## 四、第一阶段不做什么

为了避免项目失控，第一阶段明确不做：

- 聊天系统
- 复杂消息中台
- 多 namespace 乱拆
- 分布式多节点广播
- Redis Socket.IO adapter
- 数据库存储的 socket session 管理
- 高并发监控流

先把“后台模板刚需实时能力”跑通。

---

## 五、总体设计原则

### 1. 业务模块不要直接管理 socket 连接细节

建议抽一个统一实时服务层：

- `RealtimeService`
- 或 `SocketIoService`

职责：

- 统一发事件
- 统一管理 room
- 统一处理用户绑定
- 统一封装广播 / 定向推送

这样后续：

- `OnlineUserService` 只负责触发“在线状态变化”
- `NoticeService` 只负责触发“公告通知”
- 具体 socket emit 统一走实时服务

---

### 2. 以“事件”建模，不以“页面”建模

不要设计成：

- config 页面专属 socket
- online 页面专属 socket

应该设计成：

- `online.changed`
- `notice.created`
- `session.kickout`

页面只是订阅事件的消费者。

---

### 3. 先单机内存态，后续再考虑分布式

第一版允许：

- 连接关系保存在内存
- room 绑定保存在进程内
- 单实例广播

后续如果项目要上多实例，再考虑：

- Redis Pub/Sub
- Redis adapter
- 分布式会话同步

---

## 六、推荐的命名空间和房间设计

### 1. namespace

第一版建议只保留一个：

- `/admin`

好处：

- 简单
- 前后端统一
- 不会一开始就把结构拆复杂

配置建议：

```toml
[socket_io]
default_namespace = "/admin"
```

---

### 2. room

建议统一约定：

- `user:{user_id}`：给某个用户单播
- `role:{role_code}`：给某个角色广播
- `all-admin`：给全部后台在线用户广播

这样基本已经能覆盖后台常见推送需求。

---

## 七、连接与鉴权建议

这里建议分两步，不要第一版就做得很重。

### 方案选择

第一版推荐：

- 建立连接后，客户端主动发 `auth.bind`
- 服务端校验 token
- 校验通过后，把 socket 绑定到用户和角色 room

不建议第一版就做：

- connect 阶段复杂鉴权链
- 过早耦合底层握手细节

原因：

- 当前项目先把业务跑通更重要
- 后续如果框架层要增强 connect auth，再平滑升级

---

### 推荐流程

1. 前端连接 `/admin`
2. 连接成功后发送 `auth.bind`
3. 服务端解析 token，拿到：
   - `user_id`
   - `roles`
   - `device`
4. 绑定 socket 到：
   - `user:{user_id}`
   - 对应 `role:{role_code}`
   - `all-admin`
5. 返回绑定成功结果

建议事件：

- `auth.bind`
- `auth.bound`
- `auth.error`

---

## 八、事件模型规划

### 1. 客户端 -> 服务端

#### `auth.bind`

连接后的身份绑定。

建议 payload：

```json
{
  "token": "Bearer xxx"
}
```

#### `notice.read`

公告已读。

建议 payload：

```json
{
  "noticeId": 1001
}
```

#### `client.ping`

可选，业务侧保活或埋点。

---

### 2. 服务端 -> 客户端

#### `auth.bound`

绑定成功。

```json
{
  "userId": 1,
  "roles": ["R_SUPER"],
  "connected": true
}
```

#### `session.kickout`

强制下线。

```json
{
  "reason": "admin_kickout",
  "message": "当前账号已被管理员强制下线"
}
```

#### `online.changed`

在线状态有变化，前端收到后刷新在线用户列表或计数。

```json
{
  "type": "login",
  "userId": 1
}
```

#### `notice.created`

有新公告。

```json
{
  "noticeId": 1001,
  "title": "系统维护通知",
  "level": "info",
  "scope": "all"
}
```

#### `notice.unread.changed`

未读数变化。

```json
{
  "unreadCount": 3
}
```

#### `job.progress`

后台任务进度。

```json
{
  "jobId": "import-20260318-001",
  "progress": 65,
  "status": "running"
}
```

---

## 九、公告模块规划

公告模块非常适合放进第一批实时业务里，但建议也分层做。

### 1. 公告模块业务目标

至少支持：

- 发布公告
- 查看公告列表
- 上下线控制
- 指定公告可见范围
- 用户已读 / 未读
- 首页或顶部角标展示未读数
- 发布后实时推送给在线用户

---

### 2. 公告范围建议

建议公告范围支持：

- 全员公告
- 指定角色公告
- 指定用户公告

第一版可以先只做：

- 全员公告
- 指定角色公告

指定用户公告可以第二阶段再补。

---

### 3. 公告表设计建议

第一版建议至少拆两张表：

#### `sys_notice`

公告主表，建议字段：

- `id`
- `title`
- `content`
- `notice_level`
- `notice_scope`
- `enabled`
- `publish_status`
- `publish_time`
- `remark`
- `create_by`
- `create_time`
- `update_by`
- `update_time`

#### `sys_notice_user`

用户公告状态表，建议字段：

- `id`
- `notice_id`
- `user_id`
- `read_flag`
- `read_time`
- `delivery_status`
- `create_time`

说明：

- `sys_notice` 管公告定义
- `sys_notice_user` 管用户阅读状态
- 数据库层可以不加外键，保持和当前项目风格一致

---

### 4. 公告实时推送策略

发布公告时：

- 如果是全员公告，推送到 `all-admin`
- 如果是角色公告，推送到对应 `role:{role_code}`
- 如果是指定用户公告，推送到 `user:{user_id}`

同时：

- 给已在线用户发 `notice.created`
- 给目标用户更新 `notice.unread.changed`

离线用户怎么办：

- 不依赖实时通道保证最终可达
- 仍然以数据库记录为准
- 用户下次登录后通过 HTTP 接口拉取未读公告

这点很重要：

- Socket.IO 负责“实时通知”
- 数据库负责“最终一致”

---

## 十、与现有模块的结合方式

### 1. 在线用户模块

建议结合点：

- 登录成功后广播 `online.changed`
- 退出登录后广播 `online.changed`
- 强制踢下线时：
  - 推送 `session.kickout`
  - 再广播 `online.changed`

---

### 2. 认证模块

建议结合点：

- socket 连接后通过 `auth.bind` 复用现有 token 体系
- 不再额外搞一套独立实时鉴权模型

---

### 3. 公告模块

建议结合点：

- 创建 / 发布公告时调用 `RealtimeService`
- 已读公告时推送未读数变化

---

### 4. 后台任务模块

建议结合点：

- 任务状态变化时，按 `user:{user_id}` 推送任务进度

---

## 十一、建议的代码落点

下面是建议，不要求一次全部实现。

### 1. plugin

建议新增：

- `crates/app/src/plugin/realtime/`

职责：

- 注册实时配置
- 注册实时连接管理组件
- 挂载 socket 相关公共能力

---

### 2. service

建议新增：

- `crates/app/src/service/realtime_service.rs`
- 后续可再加 `notice_service.rs`

`RealtimeService` 负责：

- 用户 room 绑定
- 按用户推送
- 按角色推送
- 广播推送
- 统一事件发送入口

---

### 3. router

如果公告模块启动：

- `crates/app/src/router/sys_notice.rs`

HTTP 仍然负责：

- 公告 CRUD
- 公告列表
- 公告详情
- 未读列表
- 标记已读

Socket.IO 只负责：

- 实时推送

---

### 4. model

公告模块建议后续新增：

- `crates/model/src/entity/sys_notice.rs`
- `crates/model/src/entity/sys_notice_user.rs`
- `crates/model/src/dto/sys_notice.rs`
- `crates/model/src/vo/sys_notice.rs`

---

## 十二、推荐的分阶段实施顺序

### Phase 1：打基础

目标：

- 跑通 `/admin` namespace
- 建立 `auth.bind`
- 建立 `RealtimeService`
- 支持按用户 / 角色 / 全体推送

这一步完成后，实时基础设施就有了。

---

### Phase 2：接在线用户

目标：

- 登录 / 登出 / 踢下线时推送事件
- 在线用户页面可以实时刷新

这是最适合作为第一批真实业务接入的模块。

---

### Phase 3：接公告模块

目标：

- 完成公告表结构
- 完成公告 CRUD
- 完成未读 / 已读
- 发布公告后实时推送

---

### Phase 4：接任务进度

目标：

- 后台异步任务支持进度推送

---

## 十三、前端接入建议

前端建议统一封装一个实时客户端，不要每个页面自己 new 连接。

建议：

- 全局单例 socket 客户端
- 登录后建立连接
- 登出后主动断开
- 在 store 中维护：
  - 连接状态
  - 未读公告数
  - 最近公告

页面层只做：

- 订阅状态
- 响应事件

不要做成：

- 页面打开就各自建立多个 socket 连接

---

## 十四、边界与风险

### 1. 单实例限制

如果现在是单实例部署，当前方案没有问题。

如果以后多实例部署：

- 用户可能连到不同节点
- 单机内存 room 不再可靠
- 需要 Redis 做跨实例事件同步

但这不是第一版要解决的问题。

---

### 2. 公告推送不是最终存储

必须明确：

- 实时推送只能提升体验
- 不能代替数据库状态

因此公告一定要保留：

- HTTP 拉取接口
- 已读状态存储
- 未读数统计接口

---

### 3. 不要把所有业务都往实时里塞

适合实时的才走实时：

- 状态变化
- 通知提醒
- 任务进度

不适合实时的继续走 HTTP：

- 普通 CRUD 列表
- 大量筛选查询
- 复杂分页

---

## 十五、最终建议

当前项目的实时路线建议定为：

1. 统一采用 `Socket.IO`
2. 第一阶段只做后台模板必要实时能力
3. 先落实时基础设施，再接在线用户
4. 公告模块作为第二个重点实时业务模块推进
5. 所有实时推送以数据库状态为准，不搞“只靠 socket 的假实时”

---

## 十六、实施清单

后续可以按下面顺序逐项推进：

- [ ] 增加 `[socket_io]` 配置，默认 namespace 改为 `/admin`
- [ ] 新建实时模块目录与基础服务
- [ ] 定义 `auth.bind` / `auth.bound` / `auth.error`
- [ ] 定义 `session.kickout` / `online.changed`
- [ ] 在线用户模块接入实时广播
- [ ] 设计公告表结构
- [ ] 完成公告 CRUD 和已读状态
- [ ] 接入 `notice.created` / `notice.unread.changed`
- [ ] 前端统一封装 socket 客户端
- [ ] 登录后自动绑定，登出后自动断开

这份文档先作为总规划，后续每推进一个阶段，再拆成更细的实现任务即可。
