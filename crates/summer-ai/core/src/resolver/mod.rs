//! 运行时上下文：`AuthData` / `Endpoint` / `ServiceTarget`。
//!
//! 参考 [genai `resolver`](https://github.com/jeremychone/rust-genai/tree/main/src/resolver)
//! 的分层：
//!
//! - [`AuthData`] — 鉴权凭证（值 or env 名）
//! - [`Endpoint`] — 上游 base URL
//! - [`ServiceTarget`] — 一次调用的聚合上下文（endpoint + auth + model + headers）
//!
//! Adapter 仅通过 `&ServiceTarget` 读这些信息，**永远不持有**。

pub mod auth;
pub mod endpoint;
pub mod target;

pub use auth::AuthData;
pub use endpoint::Endpoint;
pub use target::ServiceTarget;
