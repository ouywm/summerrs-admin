//! 入口协议转换层（Ingress / Egress）。
//!
//! # 职责
//!
//! `relay/src/router/<protocol>/` 里的 handler 用 `IngressConverter` 把客户端的
//! 请求翻译成 canonical [`ChatRequest`]；收到上游响应后再把 canonical 响应
//! 翻译回客户端格式。**Adapter 只认 canonical**，从不关心入口协议。
//!
//! # ARCHITECTURE
//!
//! 详见 [ARCHITECTURE.md §4](../../../docs/ARCHITECTURE.md) + [CONVERSION_SPEC.md](../../../docs/CONVERSION_SPEC.md)。
//!
//! # 阶段
//!
//! - **P3.5a（当前）**：trait 签名 + `IngressCtx` + `StreamConvertState` 定义 + `OpenAIIngress` identity
//! - P3.5b：`ClaudeIngress` 非流式转换
//! - P3.5c：`ClaudeEgress` 流事件状态机
//! - P3.5d：`GeminiIngress` / `GeminiEgress`
//!
//! # 当前使用 `AdapterResult` 作返回值
//!
//! `core::error::AdapterError` 已经涵盖了我们需要的错误种类（序列化 / 反序列化 /
//! 头部 / 不支持），converter 不再自建 error——保持跨 crate 错误一致。

pub mod ingress;
