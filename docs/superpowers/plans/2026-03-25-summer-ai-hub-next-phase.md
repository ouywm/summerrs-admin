# Summer AI Hub Next Phase Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Continue `summer-ai-hub` from “OpenAI-compatible HTTP surface + capability fail-closed base” to “real multi-provider, resource-safe, regression-stable gateway”.

**Architecture:** Prioritize runtime correctness before new surface area. The next phase should harden resource routing consistency, complete multi-provider runtime rollout, and add integration-grade regression coverage so the large endpoint matrix is trustworthy instead of merely present in code.

**Tech Stack:** Rust, Axum, SeaORM, Redis runtime cache, reqwest, background task plugin, current `summer-ai-core` provider adapters, current `summer-ai-hub` relay/control-plane services.

---

## Recommended Priority Order

### Priority 1: Resource affinity hardening and resource-chain consistency

**Why first**

- Current docs explicitly list “资源型接口更精细的计费一致性” as an unfinished boundary.
- Current code already has `ResourceAffinityService`, but affinity records only store channel/account credentials and do not carry token/group ownership or liveness validation.
- This is the biggest correctness gap now that endpoint capability gating is in place.

**Primary files**

- Modify: `crates/summer-ai/hub/src/service/resource_affinity.rs`
- Modify: `crates/summer-ai/hub/src/router/openai_passthrough.rs`
- Modify: `crates/summer-ai/hub/src/relay/channel_router.rs`
- Test: `crates/summer-ai/hub/src/router/openai_passthrough.rs`
- Test: `crates/summer-ai/hub/src/service/resource_affinity.rs`

**Scope**

- Extend affinity record to include at least `token_id`, `group`, and validation metadata.
- Resolve affinity only when bound channel/account is still enabled/schedulable.
- Refuse stale affinity instead of blindly routing.
- Add regression coverage for resource chains:
  - `files -> vector_stores`
  - `assistants -> threads/runs`
  - `responses/{id}*`

### Priority 2: Real multi-provider rollout, not just adapter presence

**Why second**

- Docs place Anthropic/Gemini in Phase 2.
- The codebase already contains [anthropic.rs](/Volumes/990pro/code/rust/summerrs-admin/crates/summer-ai/core/src/provider/anthropic.rs) and [gemini.rs](/Volumes/990pro/code/rust/summerrs-admin/crates/summer-ai/core/src/provider/gemini.rs), so the next job is runtime rollout and validation, not just adapter existence.
- Current provider trait still defaults `responses` and `embeddings` to unsupported for non-OpenAI adapters in [mod.rs](/Volumes/990pro/code/rust/summerrs-admin/crates/summer-ai/core/src/provider/mod.rs).

**Primary files**

- Modify: `crates/summer-ai/core/src/provider/mod.rs`
- Modify: `crates/summer-ai/core/src/provider/anthropic.rs`
- Modify: `crates/summer-ai/core/src/provider/gemini.rs`
- Modify: `crates/summer-ai/hub/src/service/channel.rs`
- Modify: `crates/summer-ai/hub/src/router/openai.rs`
- Test: `crates/summer-ai/core/src/provider/anthropic.rs`
- Test: `crates/summer-ai/core/src/provider/gemini.rs`

**Scope**

- Define which endpoints Anthropic/Gemini really support in runtime terms.
- Make provider probe/test logic channel-type aware instead of implicitly chat/OpenAI-shaped only.
- Wire a small seeded real-channel path for Anthropic/Gemini and validate:
  - chat non-stream
  - chat stream
  - explicit unsupported endpoints return clean OpenAI-style failure

### Priority 3: Route health engine and channel/account remediation

**Why third**

- Reference projects emphasize routing quality after capability registry: priority, weight, retry, cooldown, disable/recover.
- Current code already has recovery loop and relay success/failure recording, so the next step is making the scheduler genuinely operational.

**Primary files**

- Modify: `crates/summer-ai/hub/src/service/channel.rs`
- Modify: `crates/summer-ai/hub/src/relay/channel_router.rs`
- Modify: `crates/summer-ai/hub/src/plugin.rs`
- Test: `crates/summer-ai/hub/src/service/channel.rs`
- Test: `crates/summer-ai/hub/src/relay/channel_router.rs`

**Scope**

- Distinguish channel-level and account-level failure/cooldown more clearly.
- Ensure retries exclude failing account/channel appropriately per attempt.
- Improve auto-recovery policy from “periodic probe” to “probe + cooldown-aware reentry”.
- Add tests for:
  - priority group selection
  - weighted selection
  - excluded channel retry
  - auto-disabled recovery path

### Priority 4: Integration-grade regression harness

**Why fourth**

- Docs Step 4 explicitly asks for enhanced tests and curl regression.
- Current `summer-ai-hub` tests are mostly helper/unit tests; there is still no real integration harness proving the broad endpoint matrix end-to-end.

**Primary files**

- Create: `crates/summer-ai/hub/tests/openai_surface.rs`
- Create: `crates/summer-ai/hub/tests/resource_chains.rs`
- Modify: `docs/summer-ai-hub-curl-regression.md`
- Modify: `docs/summer-ai-hub-endpoint-matrix.md`

**Scope**

- Add mock-upstream-driven integration tests for:
  - `chat/completions`
  - `responses`
  - one resource-create/read/update/delete chain
  - one unsupported-empty-200 case
- Keep curl doc aligned with real observed behavior.

### Priority 5: Fine-grained endpoint governance

**Why fifth**

- The endpoint fail-closed base is now in place, but governance is still incomplete.
- Schema has more policy surface than runtime currently uses.

**Primary files**

- Modify: `crates/summer-ai/hub/src/service/token.rs`
- Modify: `crates/summer-ai/hub/src/relay/billing.rs`
- Modify: `crates/summer-ai/hub/src/router/openai.rs`
- Modify: `crates/summer-ai/hub/src/router/openai_passthrough.rs`

**Scope**

- Start using `group_ratio.endpoint_scopes` where it materially affects policy.
- Normalize endpoint naming strategy so `chat`, `responses`, `assistants`, `vector_stores` stay consistent across:
  - DB config
  - routing
  - billing
  - token policy
  - docs

## Explicit Non-Priority Items

- `Realtime` WebSocket proxy: keep deferred for now.
- Frontend admin pages: keep deferred as requested.
- More endpoint expansion for its own sake: do not prioritize until the current large surface has integration confidence.

## Immediate Recommendation

If we start coding right now, the best next task is:

1. Harden `ResourceAffinityService`
2. Add resource-chain integration tests
3. Then move into real Anthropic/Gemini runtime rollout

That sequence matches the docs, matches the reference projects’ maturity path, and fits the current codebase best.
