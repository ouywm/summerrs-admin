# Summer-AI Issue Audit Report

**Date**: April 1, 2026
**Audit Scope**: Core, Hub, and Model crates.
**Methodology**: Systematic line-by-line verification of 36 identified issues against current source code.

---

## 🔴 Critical Issues Audit

| ID | Issue | Status | Verification Findings |
|---|---|---|---|
| **C-01** | SSE UTF-8 splitting | **Fixed** | `SseParser` in `core/src/types/sse_parser.rs` uses a dedicated byte-level buffer to ensure multi-byte UTF-8 sequences are never split during parsing. |
| **C-02** | RouteHealth race conditions | **Fixed** | Implementation in `hub/src/service/route_health.rs` now uses atomic Redis hash operations (`HINCRBY`) instead of the non-atomic GET-SET pattern. |
| **C-03** | Billing atomicity | **Fixed** | `hub/src/relay/billing.rs` implements a 3-phase "Pending -> Reserved -> Settled" pattern. Even if a process crashes, the "Pending/Reserved" status in Redis allows for subsequent reconciliation. |
| **C-04** | Streaming settlement fire-and-forget | **Not Fixed** | **CRITICAL**: In `hub/src/relay/stream.rs`, `settle_usage_accounting` is `await`ed directly inside the `async_stream!` block. If a client disconnects, the stream is dropped by Axum, and the settlement logic never runs. This leads to leaked "Reserved" quota that is never settled. |
| **C-05** | Multipart OOM | **Fixed** | `MAX_MULTIPART_FILE_SIZE_BYTES` and chunked processing are implemented to prevent memory exhaustion from large file uploads. |
| **C-06** | RateLimit atomicity | **Partially Fixed** | Uses `incr_with_expire` for atomicity, but rollback and cleanup logic still rely on separate `DECRBY` commands which are not bundled in a transaction or Lua script. |

---

## 🟠 High Issues Audit

| ID | Issue | Status | Verification Findings |
|---|---|---|---|
| **H-01** | Token cache 60s window | **Not Fixed** | `TokenService` caches token info for 60 seconds with no active invalidation mechanism. Changes to user quota or token status may take up to a minute to propagate. |
| **H-02** | Credential leaks | **Fixed** | Sensitive fields like `api_key` are marked with `#[serde(skip_serializing)]` in VOs and DTOs. |
| **H-03** | Panic-prone `.expect()` / `.unwrap()` | **Not Fixed** | Multiple instances of `.expect()` and `.unwrap()` remain in `hub/src/router/openai/support.rs` and provider adapters. |
| **H-04** | Request collapsing (SingleFlight) | **Not Fixed** | High-concurrency route selection for the same model/group still results in redundant Redis/DB queries instead of collapsing into a single flight. |
| **H-05** | N+1 Redis queries | **Not Fixed** | `ChannelRouter::load_schedulable_accounts` executes health checks in a loop, resulting in N Redis calls where N is the number of accounts. |
| **H-06** | LastUsedIp DB pressure | **Not Fixed** | The database is updated on every request to record the client IP, creating significant write pressure under high load. |
| **H-07** | Unreliable stream settlement | **Partially Fixed** | `resolve_stream_settlement` correctly identifies early stream termination but still yields `status_code: 0`, which triggers a generic "Overload" cooldown rather than a more specific error. |
| **H-08** | Tool arguments fallback | **Partially Fixed** | Gemini and Anthropic providers attempt JSON parsing for tool arguments but fallback to raw strings on failure, leading to inconsistent downstream processing. |

---

## 🟡 Medium Issues Audit

| ID | Issue | Status | Verification Findings |
|---|---|---|---|
| **M-01** | Model config cache stale | **Not Fixed** | Similar to H-01, model configurations are cached for 60s without invalidation. |
| **M-03** | Refund error swallowed | **Fixed** | `refund_with_retry` is implemented with proper logging and retry logic. |
| **M-06** | HTTP client timeouts | **Fixed** | `UpstreamHttpClient` now has explicit connection (10s), request (300s), and read (60s) timeouts. |
| **M-08** | Secret leakage in traces | **Partially Fixed** | While skipped in VOs, credentials may still appear in raw request/response logs if debug logging is enabled. |

---

## 🔵 Low & Systemic Issues Audit

| ID | Category | Status | Summary |
|---|---|---|---|
| **L-08** | Performance | **Not Fixed** | `ModelService::list_available` executes 4 independent DB queries instead of a single JOIN. |
| **S-01** | Consistency | **Not Fixed** | Error handling remains inconsistent across crates (Result vs Panic vs Silent failure). |
| **S-02** | Reliability | **Not Fixed** | Redis is a hard dependency; failures are not handled with grace or fallback to DB. |

---

## Summary of Changes Needed

1. **C-04 (Immediate)**: Wrap `settle_usage_accounting` in `tokio::spawn` or a `Drop` guard in `stream.rs` to ensure settlement runs even if the client disconnects.
2. **H-03 (Immediate)**: Refactor `hub/src/router/openai/support.rs` to replace all `.unwrap()` and `.expect()` with proper `Result` handling.
3. **H-05 (Optimization)**: Batch Redis `MGET` or `HMGET` calls for account health checks in `channel_router.rs`.
4. **C-06 (Atomicity)**: Implement Lua scripts for RateLimit operations to ensure full atomicity.
