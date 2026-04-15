# Rate Limit（限流）

本仓库提供两层能力：

- 运行时限流引擎：`summer_common::rate_limit::{RateLimitEngine, RateLimitContext}`
- handler 属性宏：`#[rate_limit(...)]`（来自 `summer_admin_macros`）

`#[rate_limit]` 的目标是：**把限流声明写在接口上**，无需在 handler 内手写限流逻辑。

## 1. 使用方式

在 handler 上添加宏（需配合 `summer-web` 的路由宏使用）：

```rust,ignore
use summer_admin_macros::rate_limit;
use summer_common::error::ApiResult;
use summer_web::get_api;

// 单 IP 每秒 2 次
#[rate_limit(rate = 2, per = "second", key = "ip")]
#[get_api("/limited")]
async fn limited_handler() -> ApiResult<()> {
    Ok(())
}
```

## 2. 前置条件：提供 RateLimitEngine

`#[rate_limit]` 会注入 `RateLimitContext` extractor，而 `RateLimitContext` 需要从请求中拿到 `RateLimitEngine`。

两种方式二选一：

1) **axum layer 注入**

```rust,ignore
use summer_common::rate_limit::RateLimitEngine;
use summer_web::axum::Extension;

let router = router.layer(Extension(RateLimitEngine::new(None)));
```

2) **summer 组件注册**

```rust,ignore
use summer_common::rate_limit::RateLimitEngine;

app.add_component(RateLimitEngine::new(None));
```

> `RateLimitEngine::new(Some(redis))` 可启用 Redis 后端（取决于你的应用是否提供 `summer_redis::Redis`）。

## 3. 参数说明

语法：

```plain
#[rate_limit(rate = <u64>, per = "second|minute|hour|day", ...)]
```

必填：

- `rate`: 每个窗口允许的请求数
- `per`: 窗口大小，支持 `"second" | "minute" | "hour" | "day"`

可选：

- `key`：默认 `"global"`
  - `"global"`：全局（所有请求共享）
  - `"ip"`：按客户端 IP
  - `"user"`：按用户（需要 `summer-auth` 把 `UserSession` 注入到 `request.extensions`；未登录会回退到 IP）
  - `"header:<name>"`：按 Header 值（缺失时用 `"unknown"`）
- `backend`：默认 `"memory"`，支持 `"memory" | "redis"`
- `algorithm`：默认 `"token_bucket"`，支持
  - `"token_bucket" | "fixed_window" | "sliding_window" | "leaky_bucket" | "throttle_queue"`
- `failure_policy`：默认 `"fail_open"`，支持
  - `"fail_open" | "fail_closed" | "fallback_memory"`
- `burst`：仅 `token_bucket` 支持；默认等于 `rate`
- `max_wait_ms`：仅 `throttle_queue` 支持且必须提供；最大排队等待时间（毫秒）
- `message`：被限流时返回的提示文案（默认 `"请求过于频繁"`）

## 4. 示例：排队限流（throttle_queue）

```rust,ignore
use summer_admin_macros::rate_limit;
use summer_common::error::ApiResult;
use summer_web::get_api;

// 每秒 1 次，最多等 1500ms；超过则返回 429
#[rate_limit(rate = 1, per = "second", key = "ip", algorithm = "throttle_queue", max_wait_ms = 1500)]
#[get_api("/throttle-queue")]
async fn handler() -> ApiResult<()> {
    Ok(())
}
```

