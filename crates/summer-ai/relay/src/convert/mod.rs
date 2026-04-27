//! 入口协议转换层（Ingress / Egress）。
//!
//! # 职责
//!
//! `relay/src/router/<protocol>/` 里的 handler 用 `IngressConverter` 把客户端的
//! 请求翻译成 canonical [`ChatRequest`]；收到上游响应后再把 canonical 响应
//! 翻译回客户端格式。**Adapter 只认 canonical**，从不关心入口协议。
//!
//! # 阶段
//!
//! - **当前**：trait 签名 + `IngressCtx` + `StreamConvertState` 定义
//!   + `OpenAIIngress` identity + `ClaudeIngress` 非流式
//! - 后续：`ClaudeIngress` 流事件状态机、`GeminiIngress` / `GeminiEgress`
//!
//! # 当前使用 `AdapterResult` 作返回值
//!
//! `core::error::AdapterError` 已经涵盖了我们需要的错误种类（序列化 / 反序列化 /
//! 头部 / 不支持），converter 不再自建 error——保持跨 crate 错误一致。

pub mod ingress;
