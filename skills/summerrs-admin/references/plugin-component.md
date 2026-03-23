# Plugin And Component Patterns

这部分主要来自 Summer 文档，并结合本仓库当前插件分层：`app` 只装配，system / plugins / mcp 承担真实插件实现。

## Canonical examples

- system 权限插件：`crates/summer-system/src/plugins/perm_bitmap.rs`
- system schema sync：`crates/summer-system/src/plugins/schema_sync.rs`
- system socket 网关：`crates/summer-system/src/plugins/socket_gateway.rs`
- MCP 嵌入插件：`crates/summer-mcp/src/plugin.rs`
- 通用插件：`crates/summer-plugins/src/log_batch_collector/mod.rs`
- 应用装配入口：`crates/app/src/main.rs`

## 先分清三种东西

### 1. Config

读取 TOML 配置的结构体：

```rust
#[derive(Debug, Deserialize, Configurable)]
#[config_prefix = "background-task"]
pub struct BackgroundTaskConfig {
    pub capacity: usize,
    pub workers: usize,
}
```

### 2. Component

运行时可注入、可克隆的对象，例如：

- `DbConn`
- `SessionManager`
- 自定义客户端
- 业务组件

### 3. Plugin

负责在应用启动时：

- 读配置
- 初始化资源
- 注册组件
- 挂 router layer
- 设置依赖顺序

## 当前仓库的分层约定

- `crates/app/src/main.rs`：只负责 `App::new()` 和 `.add_plugin(...)`
- `crates/summer-system/src/plugins`：system 相关插件
- `crates/summer-plugins/src/*`：通用基础设施插件
- `crates/summer-mcp/src/plugin.rs`：MCP 相关插件

如果只是新增业务能力，不要默认去改 `crates/app`；优先看是否应该落在 `summer-system` 或 `summer-plugins`。

## 自定义 Plugin 标准模板

```rust
pub struct MyPlugin;

#[async_trait]
impl Plugin for MyPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app.get_config::<MyConfig>().expect("load config failed");
        let component = MyComponent::new(config);
        app.add_component(component);
    }

    fn name(&self) -> &str {
        "my-plugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec![]
    }
}
```

## 什么时候实现 `dependencies()`

当插件依赖别的插件先把组件准备好时，就实现它。

当前仓库例子：

- `SystemSchemaSyncPlugin` 依赖 `SeaOrmPlugin`
- `PermBitmapPlugin` 依赖 `SeaOrmPlugin` 和 `SummerAuthPlugin`
- `SocketGatewayPlugin` 依赖 `WebPlugin`
- `McpPlugin` 依赖 `SeaOrmPlugin`

## `Service` 怎么看

`Service` 是可注入组件的一种常用写法：

```rust
#[derive(Clone, Service)]
pub struct UserService {
    #[inject(component)]
    db: DbConn,
    #[inject(config)]
    config: UserConfig,
}
```

### 支持的注入模式

- `#[inject(component)] foo: T`
- `#[inject(component)] foo: Option<T>`
- `#[inject(config)] conf: T`
- `LazyComponent<T>` 解决循环依赖

## 组件注册与读取

### 注册

```rust
app.add_component(my_component);
```

### 读取

```rust
let db: DbConn = app.get_component::<DbConn>().expect("...");
let config = app.get_config::<MyConfig>().expect("...");
```

只在 `Plugin::build()` 里直接碰容器；业务代码优先靠 `Service` 注入。

## 常见模式

### 1. 插件里挂 router layer

参考：`crates/summer-mcp/src/plugin.rs`

适合：

- 自定义中间件
- 统一子路由挂载
- streamable http / embedded 服务嵌入

### 2. 插件里启动后台任务

参考：`crates/summer-plugins/src/log_batch_collector/mod.rs`

适合：

- 后台消费器
- 定时清理器
- 缓存刷新器

## 新增插件的最小流程

1. 先定义配置结构体（如果需要）
2. 再决定是 `Plugin` 还是简单 `#[component]`
3. 在 `build()` 里读配置、取依赖、注册组件或挂 layer
4. 如有顺序依赖，补 `dependencies()`
5. 回到 `crates/app/src/main.rs` 注册插件

## 反模式

- 不要把大量业务逻辑塞进 `Plugin::build()`
- 不要在业务 route 里到处 `get_component()`
- 不要把 `crates/app` 变成业务实现层
