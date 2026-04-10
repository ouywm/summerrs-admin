# Fixed Tenant HTTP Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `summer-sharding` 增加一套可复用的 Axum 全局租户中间件与提取器，第一版对所有 HTTP 请求写死同一个 `TenantContext`，并真正驱动 `with_tenant(...)` 进入 SQL 路由链。

**Architecture:** 在 `summer-sharding` 内新增一个 `web` 集成模块，提供 `TenantContextLayer`、`CurrentTenant`、`OptionalCurrentTenant`。中间件在请求入口同时写入 `request.extensions` 和 task-local `CURRENT_TENANT`，这样 handler 和 `ShardingConnection` 都能消费同一份租户上下文；`app` 侧只需全局挂载一层。第一版明确只支持固定租户，不从 header/JWT/query 取值，并在代码里留下后续扩展 TODO。

**Tech Stack:** `summer-sharding`, `summer-web`(axum re-export), `tower-layer`, `tower-service`, `tokio`, `summer-auth`, `summer-common`

---

### Task 1: Add Web Integration Surface

**Files:**
- Modify: `crates/summer-sharding/Cargo.toml`
- Modify: `crates/summer-sharding/src/lib.rs`
- Create: `crates/summer-sharding/src/web/mod.rs`

- [ ] **Step 1: Add the failing API-surface test targets**

Document the intended public API in compile-focused tests inside the new module:
- `TenantContextLayer::new()`
- `CurrentTenant` extractor
- `OptionalCurrentTenant` extractor

- [ ] **Step 2: Run the targeted crate tests to verify missing symbols fail**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding web::`
Expected: FAIL with unresolved module / unresolved item errors

- [ ] **Step 3: Add the minimal dependency and export scaffolding**

Add:
- `summer-web = { workspace = true, optional = true }`
- `tower-layer = { version = "0.3", optional = true }`
- `tower-service = { version = "0.3", optional = true }`
- crate feature `web = ["summer-web", "tower-layer", "tower-service"]`

Wire exports from `lib.rs` behind the `web` feature.

- [ ] **Step 4: Run the targeted tests to verify the API surface now compiles**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding web::`
Expected: tests compile further and then fail on missing runtime behavior

### Task 2: Implement Fixed Tenant Middleware With TDD

**Files:**
- Create: `crates/summer-sharding/src/web/middleware.rs`
- Modify: `crates/summer-sharding/src/web/mod.rs`

- [ ] **Step 1: Write failing middleware behavior tests**

Add tests for:
- middleware inserts `TenantContext` into request extensions
- middleware wraps downstream in `with_tenant(...)`
- middleware uses the fixed context for every request

- [ ] **Step 2: Run the exact test subset to verify RED**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding fixed_tenant_`
Expected: FAIL because layer/service behavior is not implemented

- [ ] **Step 3: Implement minimal middleware**

Implement a layer/service pair similar to `summer-auth::AuthLayer`, but:
- never reads headers/JWT/query
- always clones a fixed `TenantContext`
- stores it in `req.extensions_mut()`
- executes downstream inside `with_tenant(tenant, ...)`
- add TODO comments for future header/JWT/query support

- [ ] **Step 4: Re-run the middleware tests to verify GREEN**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding fixed_tenant_`
Expected: PASS

### Task 3: Implement Tenant Extractors With TDD

**Files:**
- Create: `crates/summer-sharding/src/web/extractor.rs`
- Modify: `crates/summer-sharding/src/web/mod.rs`

- [ ] **Step 1: Write failing extractor tests**

Add tests for:
- `CurrentTenant` returns the inserted `TenantContext`
- `OptionalCurrentTenant` returns `Some(...)` when present
- `OptionalCurrentTenant` returns `None` when absent
- `CurrentTenant` rejects when absent

- [ ] **Step 2: Run the extractor subset to verify RED**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding current_tenant`
Expected: FAIL because extractors are missing

- [ ] **Step 3: Implement minimal extractors**

Use `FromRequestParts`, reading from `parts.extensions`, matching existing project conventions from `summer-auth` and `summer-common`.

- [ ] **Step 4: Re-run the extractor tests to verify GREEN**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding current_tenant`
Expected: PASS

### Task 4: Integrate Into App Router Layer

**Files:**
- Modify: `crates/app/Cargo.toml`
- Modify: `crates/app/src/main.rs`

- [ ] **Step 1: Write a failing app-level integration test**

Add a focused test around the app/router setup proving the fixed tenant layer is mounted globally with the expected hard-coded tenant ID.

- [ ] **Step 2: Run the targeted app test to verify RED**

Run: `CARGO_NET_OFFLINE=true cargo test -p app fixed_tenant`
Expected: FAIL because the layer is not registered yet

- [ ] **Step 3: Register the global layer**

Mount `TenantContextLayer` from `app.add_router_layer(...)` for all HTTP routes.

- [ ] **Step 4: Re-run the targeted app test to verify GREEN**

Run: `CARGO_NET_OFFLINE=true cargo test -p app fixed_tenant`
Expected: PASS unless blocked by unrelated workspace dependency issues

### Task 5: Verify Sharding Consumption Path

**Files:**
- Modify: `crates/summer-sharding/src/web/middleware.rs`
- Modify: `crates/summer-sharding/src/connector/connection.rs` (tests only if needed)

- [ ] **Step 1: Write the failing end-to-end sharding behavior test**

Add a test proving that code executed inside the middleware-scoped request causes shared-row SQL to include the fixed `tenant_id`.

- [ ] **Step 2: Run the targeted behavior test to verify RED**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding tenant_http_scope`
Expected: FAIL if task-local propagation is not correctly wired

- [ ] **Step 3: Adjust integration glue minimally**

Only add the code required to bridge middleware scope to the already-existing `TenantRouter` / `ShardingConnection` pipeline.

- [ ] **Step 4: Re-run the targeted behavior test to verify GREEN**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding tenant_http_scope`
Expected: PASS

### Task 6: Final Verification

**Files:**
- Verify only

- [ ] **Step 1: Run formatting**

Run: `cargo fmt --package summer-sharding --package app`
Expected: exit 0

- [ ] **Step 2: Run summer-sharding tests**

Run: `CARGO_NET_OFFLINE=true cargo test -p summer-sharding --features web`
Expected: PASS

- [ ] **Step 3: Run app tests if feasible**

Run: `CARGO_NET_OFFLINE=true cargo test -p app`
Expected: PASS, or document unrelated pre-existing failures precisely

- [ ] **Step 4: Re-read against approved design**

Checklist:
- fixed tenant only
- global HTTP layer
- request extension + task-local both wired
- extractor available
- TODOs for header/JWT/query future work
