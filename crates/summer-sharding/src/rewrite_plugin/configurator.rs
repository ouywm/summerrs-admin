use summer::app::AppBuilder;
use summer::plugin::MutableComponentRegistry;

use summer_sql_rewrite::PluginRegistry;

/// 扩展 `AppBuilder`，提供 SQL 改写插件注册入口。
///
/// 与 `SummerAuthConfigurator` 同一模式：
/// 在应用层 `main.rs` 中通过链式调用注册插件，
/// 而非修改 `summer-sharding` 库内部代码。
///
/// # 示例
///
/// ```rust,ignore
/// use summer::App;
/// use summer_sharding::{SummerShardingPlugin, ShardingRewriteConfigurator};
///
/// App::new()
///     .add_plugin(SummerShardingPlugin)
///     .sharding_rewrite_configure(|registry| {
///         registry
///             .register(MyPlugin1::new())
///             .register(MyPlugin2::new())
///     })
///     .run()
///     .await;
/// ```
pub trait ShardingRewriteConfigurator {
    /// 注册 SQL 改写插件。
    ///
    /// 接收一个闭包，闭包参数为 `&mut PluginRegistry`，
    /// 在闭包内通过 `registry.register(...)` 注册插件。
    fn sharding_rewrite_configure<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut PluginRegistry) -> &mut PluginRegistry;
}

impl ShardingRewriteConfigurator for AppBuilder {
    fn sharding_rewrite_configure<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut PluginRegistry) -> &mut PluginRegistry,
    {
        let mut registry = PluginRegistry::new();
        f(&mut registry);

        if !registry.is_empty() {
            self.add_component(registry)
        } else {
            self
        }
    }
}
