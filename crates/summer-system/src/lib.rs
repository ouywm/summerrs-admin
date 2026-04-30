pub mod job;
pub mod plugins;
pub mod router;
pub mod service;
pub mod socketio;

pub use router::router_with_layers;

pub fn system_group() -> &'static str {
    env!("CARGO_PKG_NAME")
}

// 初始化 rust-i18n（编译期加载工作区根目录 `locales/` 下的翻译文件到二进制）
rust_i18n::i18n!("../../locales", fallback = "en");
