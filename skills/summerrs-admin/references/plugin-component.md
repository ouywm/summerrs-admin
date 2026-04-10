# Plugin And Component Patterns

This reference explains how plugins, components, and configuration are layered in
the current workspace.

## Canonical Examples

- System permission plugin: `crates/summer-system/src/plugins/perm_bitmap.rs`
- System Socket gateway plugin: `crates/summer-system/src/plugins/socket_gateway.rs`
- Shared schema sync plugin: `crates/summer-plugins/src/entity_schema_sync.rs`
- Shared infrastructure plugins: `crates/summer-plugins/src/*`
- MCP plugin: `crates/summer-mcp/src/plugin.rs`
- AI hub plugin: `crates/summer-ai/hub/src/plugin.rs`
- Rig plugin: `crates/summer-rig/src/plugin.rs`
- App assembly root: `crates/app/src/main.rs`

## Three Different Concepts

### 1. Config

TOML-backed configuration structs:

```rust
#[derive(Debug, Deserialize, Configurable)]
#[config_prefix = "background-task"]
pub struct BackgroundTaskConfig {
    pub capacity: usize,
    pub workers: usize,
}
```

### 2. Component

Cloneable runtime objects that can be injected:

- `DbConn`
- `SessionManager`
- custom clients
- registries
- service dependencies

### 3. Plugin

Startup-time wiring that can:

- read config
- create runtime resources
- register components
- attach router layers
- declare startup dependencies

## Current Layering Rules

- `crates/app/src/main.rs` is the assembly root for `.add_plugin(...)`
- `crates/summer-system/src/plugins` contains system-specific plugins
- `crates/summer-plugins/src/*` contains shared infrastructure plugins
- `crates/summer-mcp/src/plugin.rs` owns MCP plugin wiring
- `crates/summer-ai/hub/src/plugin.rs` owns AI hub runtime wiring
- `crates/summer-rig/src/plugin.rs` owns Rig-related provider wiring

If the work is just "add business behavior", do not default to `crates/app`.
Decide first whether it belongs in `summer-system`, `summer-plugins`, or another
dedicated crate.

## Standard Plugin Template

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

## When To Implement `dependencies()`

Use `dependencies()` when the plugin needs another plugin to register components first.

Common cases in this repo:

- `SocketGatewayPlugin` depends on `WebPlugin`
- Plugins that need `DbConn` depend on `SeaOrmPlugin`
- Plugins that need auth components depend on `SummerAuthPlugin`
- `EntitySchemaSyncPlugin` is registered early because it needs database access

## Service Injection Pattern

```rust
#[derive(Clone, Service)]
pub struct UserService {
    #[inject(component)]
    db: DbConn,
    #[inject(config)]
    config: UserConfig,
}
```

Supported injection patterns commonly used here:

- `#[inject(component)] foo: T`
- `#[inject(component)] foo: Option<T>`
- `#[inject(config)] conf: T`
- `LazyComponent<T>` when circular dependency pressure appears

## Component Registration And Lookup

Register inside plugin build:

```rust
app.add_component(my_component);
```

Lookup inside plugin build:

```rust
let db: DbConn = app.get_component::<DbConn>().expect("...");
let config = app.get_config::<MyConfig>().expect("...");
```

Outside plugin build, prefer service injection over manual container lookups.

## Common Patterns

### Attach Router Layers In Plugins

See `crates/summer-mcp/src/plugin.rs`.

Use this when you need:

- custom middleware
- embedded sub-services
- HTTP transport attachment

### Start Background Tasks In Plugins

See `crates/summer-plugins/src/log_batch_collector/mod.rs`.

Use this when you need:

- background consumers
- cleanup jobs
- cache refreshers

## Minimal Flow For A New Plugin

1. Define a config struct if the plugin needs configuration
2. Decide whether the new runtime object should be a plugin or just a component
3. In `build()`, read config, resolve dependencies, register components, and/or
   attach layers
4. Add `dependencies()` if startup order matters
5. Register the plugin in `crates/app/src/main.rs`

## Anti-Patterns

- Do not put heavy business logic in `Plugin::build()`
- Do not call `get_component()` everywhere in business routes
- Do not turn `crates/app` into a business implementation crate
