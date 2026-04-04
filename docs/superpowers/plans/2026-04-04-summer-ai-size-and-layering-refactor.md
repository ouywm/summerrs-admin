# Summer-ai Size And Layering Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce every Rust file over 1500 lines in `crates/summer-ai`, while enforcing that routers no longer own business logic.

**Architecture:** First extract neutral OpenAI relay helpers, then move `chat`, `responses`, and `embeddings` business flows from router into service modules, followed by `openai_passthrough` and the oversized service/relay/provider files. Each phase keeps behavior stable and is validated with focused regression tests plus library-wide checks.

**Tech Stack:** Rust, Axum via `summer-web`, SeaORM, Reqwest, Tokio, cargo test, cargo clippy

---

### Task 1: Establish The Refactor Baseline

**Files:**
- Modify: `docs/superpowers/specs/2026-04-04-summer-ai-size-and-layering-design.md`
- Modify: `docs/superpowers/plans/2026-04-04-summer-ai-size-and-layering-refactor.md`

- [ ] **Step 1: Confirm the current oversized-file inventory**

Run:

```bash
find crates/summer-ai -name '*.rs' -print0 | xargs -0 wc -l | sort -nr | head -30
```

Expected: the current oversized files list includes `router/openai.rs`, `router/openai_passthrough.rs`, `service/channel.rs`, `service/log.rs`, `relay/channel_router.rs`, `router/test_support.rs`, `core/provider/gemini.rs`, and `core/provider/anthropic.rs`.

- [ ] **Step 2: Confirm the repo is clean before the refactor slice**

Run:

```bash
git status --short
```

Expected: no unexpected modifications outside the files for the current slice.

### Task 2: Move `router/openai.rs` Business Logic Into Service Modules

**Files:**
- Create: `crates/summer-ai/hub/src/service/openai_http.rs`
- Create: `crates/summer-ai/hub/src/service/openai_chat_relay.rs`
- Create: `crates/summer-ai/hub/src/service/openai_responses_relay.rs`
- Create: `crates/summer-ai/hub/src/service/openai_embeddings_relay.rs`
- Modify: `crates/summer-ai/hub/src/service/mod.rs`
- Modify: `crates/summer-ai/hub/src/router/openai.rs`
- Modify: `crates/summer-ai/hub/src/router/openai_passthrough.rs`
- Test: `crates/summer-ai/hub/src/router/openai/tests/mock_upstream.rs`

- [ ] **Step 1: Write the failing regression tests for thin router wrappers**

Add/expand tests in:

```rust
#[tokio::test]
fn responses_route_persists_request_and_execution_snapshots() { /* ... */ }

#[tokio::test]
fn responses_stream_route_persists_request_and_execution_snapshots() { /* ... */ }

#[tokio::test]
fn embeddings_route_persists_request_and_execution_snapshots() { /* ... */ }
```

Expected red state: after moving the route wrappers to delegate to new services, these tests protect the existing flow and persistence points.

- [ ] **Step 2: Extract shared OpenAI relay HTTP helpers**

Move these out of `router/openai.rs`:

```rust
extract_request_id(...)
extract_upstream_request_id(...)
insert_request_id_header(...)
insert_upstream_request_id_header(...)
fallback_usage(...)
```

Place them into `service/openai_http.rs`, then update imports in both `router/openai.rs` and `router/openai_passthrough.rs`.

- [ ] **Step 3: Extract chat relay flow into `service/openai_chat_relay.rs`**

Move the business flow for:

```rust
chat_completions(...)
```

The router should become a thin wrapper that only extracts request data and calls the service.

- [ ] **Step 4: Extract responses relay flow into `service/openai_responses_relay.rs`**

Move the business flow for:

```rust
responses(...)
build_responses_stream_response(...)
build_chat_bridged_responses_stream_response(...)
ResponsesStreamTracker
```

- [ ] **Step 5: Extract embeddings relay flow into `service/openai_embeddings_relay.rs`**

Move the business flow for:

```rust
embeddings(...)
```

- [ ] **Step 6: Re-run the focused router regression tests**

Run:

```bash
cargo test -p summer-ai-hub --lib persists_request_and_execution_snapshots -- --ignored --test-threads=1
```

Expected: if local Postgres/Redis are available, these ignored tests pass; if the environment cannot serve them, the compilation still succeeds and the failure mode is environment-only rather than compile/runtime regressions.

- [ ] **Step 7: Run the main validation suite**

Run:

```bash
cargo test -p summer-ai-hub --lib
```

Expected: pass with zero failures.

- [ ] **Step 8: Run lint validation**

Run:

```bash
cargo clippy -p summer-ai-hub --lib --tests -- -D warnings
```

Expected: pass with zero warnings.

- [ ] **Step 9: Confirm `router/openai.rs` is below the limit**

Run:

```bash
wc -l crates/summer-ai/hub/src/router/openai.rs
```

Expected: line count at or below `1500`.

- [ ] **Step 10: Commit**

```bash
git add crates/summer-ai/hub/src/service/openai_http.rs \
  crates/summer-ai/hub/src/service/openai_chat_relay.rs \
  crates/summer-ai/hub/src/service/openai_responses_relay.rs \
  crates/summer-ai/hub/src/service/openai_embeddings_relay.rs \
  crates/summer-ai/hub/src/service/mod.rs \
  crates/summer-ai/hub/src/router/openai.rs \
  crates/summer-ai/hub/src/router/openai_passthrough.rs \
  crates/summer-ai/hub/src/router/openai/tests/mock_upstream.rs
git commit -m "refactor: move openai router flows into services"
```

### Task 3: Split `router/openai_passthrough.rs`

**Files:**
- Create: `crates/summer-ai/hub/src/service/openai_passthrough_relay.rs`
- Create: `crates/summer-ai/hub/src/service/openai_passthrough_multipart.rs`
- Create: `crates/summer-ai/hub/src/service/openai_passthrough_affinity.rs`
- Modify: `crates/summer-ai/hub/src/router/openai_passthrough.rs`
- Test: `crates/summer-ai/hub/src/router/openai_passthrough.rs`

- [ ] **Step 1: Write or extend failing regression tests around resource affinity chains**
- [ ] **Step 2: Move multipart parsing and body forwarding logic into service modules**
- [ ] **Step 3: Move resource routing / affinity resolution into service modules**
- [ ] **Step 4: Keep only endpoint mapping and wrapper logic in the router**
- [ ] **Step 5: Verify `router/openai_passthrough.rs <= 1500`**
- [ ] **Step 6: Run `cargo test -p summer-ai-hub --lib`**
- [ ] **Step 7: Run `cargo clippy -p summer-ai-hub --lib --tests -- -D warnings`**
- [ ] **Step 8: Commit with `refactor: split openai passthrough router`**

### Task 4: Split Oversized Hub Service And Relay Files

**Files:**
- Modify/Create around:
  - `crates/summer-ai/hub/src/service/channel.rs`
  - `crates/summer-ai/hub/src/service/log.rs`
  - `crates/summer-ai/hub/src/relay/channel_router.rs`
  - `crates/summer-ai/hub/src/router/test_support.rs`

- [ ] **Step 1: Split `channel.rs` into CRUD / probe / health / ability-sync**
- [ ] **Step 2: Split `log.rs` into query / dashboard / mapper**
- [ ] **Step 3: Split `channel_router.rs` into cache / selection / scoring**
- [ ] **Step 4: Split `test_support.rs` into fixture / db_wait / http / cleanup**
- [ ] **Step 5: Run `cargo test -p summer-ai-hub --lib`**
- [ ] **Step 6: Run `cargo clippy -p summer-ai-hub --lib --tests -- -D warnings`**
- [ ] **Step 7: Confirm each touched file is `<= 1500` lines**
- [ ] **Step 8: Commit with `refactor: split oversized hub modules`**

### Task 5: Split Oversized Provider And Test Files

**Files:**
- Modify/Create around:
  - `crates/summer-ai/core/src/provider/gemini.rs`
  - `crates/summer-ai/core/src/provider/anthropic.rs`
  - `crates/summer-ai/hub/src/router/openai/tests/mock_upstream.rs`

- [ ] **Step 1: Split provider files by request building / response parsing / stream parsing**
- [ ] **Step 2: Split `mock_upstream.rs` by endpoint family**
- [ ] **Step 3: Run `cargo test -p summer-ai-core --lib` if applicable**
- [ ] **Step 4: Run `cargo test -p summer-ai-hub --lib`**
- [ ] **Step 5: Run `cargo clippy -p summer-ai-hub --lib --tests -- -D warnings`**
- [ ] **Step 6: Confirm every remaining file in `crates/summer-ai` is `<= 1500` lines**
- [ ] **Step 7: Commit with `refactor: split oversized provider and test modules`**
