# Summer AI Hub Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the highest-priority remaining `summer-ai-hub` backend work by hardening resource affinity, tightening provider/runtime failure semantics, and keeping docs plus regressions aligned with the real system state.

**Architecture:** Keep the current chat/responses/embeddings plus passthrough surface, but shift effort away from adding new endpoints and toward correctness. The main approach is to make resource affinity safer against stale bindings, strengthen real-provider and unsupported-endpoint error normalization, and capture those guarantees in regression tests and curl docs.

**Tech Stack:** Rust, Axum, reqwest, SeaORM, Redis runtime cache, existing `summer-ai-core` adapters, existing `summer-ai-hub` routing/billing/logging stack.

---

### Task 1: Resource Affinity Hardening

**Files:**
- Modify: `crates/summer-ai/hub/src/service/resource_affinity.rs`
- Modify: `crates/summer-ai/hub/src/router/openai_passthrough.rs`
- Test: `crates/summer-ai/hub/src/service/resource_affinity.rs`
- Test: `crates/summer-ai/hub/src/router/openai_passthrough.rs`

- [x] **Step 1: Add failing tests for stale affinity snapshot handling**
- [x] **Step 2: Extend affinity records with channel snapshot metadata**
- [x] **Step 3: Reject stale affinity when channel identity changes**
- [x] **Step 4: Run targeted resource affinity tests**

### Task 2: Provider Runtime Failure Semantics

**Files:**
- Modify: `crates/summer-ai/core/src/provider/mod.rs`
- Modify: `crates/summer-ai/core/src/provider/gemini.rs`
- Modify: `crates/summer-ai/hub/src/router/openai.rs`
- Test: `crates/summer-ai/core/src/provider/mod.rs`
- Test: `crates/summer-ai/core/src/provider/gemini.rs`
- Test: `crates/summer-ai/hub/src/router/openai.rs`

- [x] **Step 1: Lock real upstream error shapes into tests**
- [x] **Step 2: Normalize unsupported endpoint / provider error mapping**
- [x] **Step 3: Verify Anthropic/Gemini stream failure regressions**

### Task 3: Route-Level Regression and Docs Sync

**Files:**
- Modify: `docs/summer-ai-hub-curl-regression.md`
- Modify: `docs/summer-ai-hub-endpoint-matrix.md`
- Test: `crates/summer-ai/hub/src/router/openai_passthrough.rs`
- Test: `crates/summer-ai/hub/src/service/model.rs`

- [x] **Step 1: Add regression coverage for current resource-chain helpers**
- [x] **Step 2: Record real upstream debugging notes in curl docs**
- [x] **Step 3: Update endpoint matrix notes if implementation moved ahead/behind**

### Task 4: Verification

**Files:**
- Verify: `crates/summer-ai/core`
- Verify: `crates/summer-ai/model`
- Verify: `crates/summer-ai/hub`

- [x] **Step 1: Run `cargo test -p summer-ai-core -p summer-ai-model -p summer-ai-hub`**
- [x] **Step 2: Run `cargo clippy -p summer-ai-core -p summer-ai-model -p summer-ai-hub --tests -- -D warnings`**
