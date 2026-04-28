//! summer-ai
//!
//! LLM 中转网关主 crate —— 把 5 个 sub-crate 汇聚到一个顶层 namespace。
//!
//! # 使用
//!
//! 推荐在 `crates/app/src/main.rs` 里**逐个注册子插件**，由框架按各自
//! `dependencies()` 声明的顺序解析（`SummerAiRelayPlugin` 依赖 `SeaOrmPlugin` +
//! `RedisPlugin`，必须在它们之后 build）：
//!
//! ```ignore
//! use summer_ai::{SummerAiAdminPlugin, SummerAiBillingPlugin, SummerAiRelayPlugin};
//!
//! App::new()
//!     .add_plugin(SeaOrmPlugin)
//!     .add_plugin(RedisPlugin)
//!     .add_plugin(SummerAiRelayPlugin)
//!     .add_plugin(SummerAiAdminPlugin)
//!     .add_plugin(SummerAiBillingPlugin)
//!     // ...
//! ```
//!
//! 之前版本里有一个"门面"`SummerAiPlugin` 在自己的 `build` 里**递归**调用 3 个
//! 子 Plugin 的 `build`。那种写法会绕过框架对 `dependencies()` 的拓扑排序
//! —— 子插件声明"我依赖 SeaOrmPlugin"在递归调用里是不生效的，结果 `DbConn`
//! 还没就绪就去 `get_component`，运行时 panic。所以这个门面已经移除。
//!
//! # 子 crate
//!
//! - [`summer_ai_core`] — 协议层（canonical 类型 + Adapter trait + 多 adapter 实现）
//! - [`summer_ai_model`] — DB Entity（SeaORM）
//! - [`summer_ai_relay`] — 运行时（/v1/* 路由 + 鉴权 + 计费前置）
//! - [`summer_ai_admin`] — 后台（/admin/ai/* CRUD）
//! - [`summer_ai_billing`] — 计费引擎

pub use summer_ai_admin::SummerAiAdminPlugin;
pub use summer_ai_billing::SummerAiBillingPlugin;
pub use summer_ai_core;
pub use summer_ai_model;
pub use summer_ai_relay::SummerAiRelayPlugin;
