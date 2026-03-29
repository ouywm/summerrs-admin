# Token Endpoint Scope Enforcement Design

## Goal
Preserve token-level endpoint scope intent by keeping the allowed scopes next to the token metadata and exposing a small runtime guard so handlers can refuse requests whose scope is not explicitly permitted.

## Context
- Token metadata already includes an `endpoint_scopes` JSON array which is cached as `TokenInfo.endpoint_scopes` after validation.
- Handlers today rely on `TokenInfo.ensure_model_allowed` but there is no analogous guard for endpoint scopes.
- Router/channel/billing files are off-limits for this change, so the enforcement must live inside the token service layer and surface a public check that downstream code can call.

## Approach
1. Populate `TokenInfo.endpoint_scopes` during token validation (already happening) and add a new `ensure_endpoint_scope_allowed(&self, scope: &str)` method that mirrors the existing model guard semantics: empty scope or empty allow-list means no restriction, otherwise fail fast with `ApiErrors::Forbidden` when the scope is missing.
2. Keep any new helpers next to `TokenInfo` so that handlers can call `token_info.ensure_endpoint_scope_allowed("chat")` immediately after validation. No router changes are required within this change window.
3. Add focused unit tests near the token service to verify the new guard honors empty lists, scopes that match, and disallowed scopes.

## Testing
- Add `#[cfg(test)] mod tests` in `service/token.rs` exercising `ensure_endpoint_scope_allowed`.
- Run `cargo test -p summer-ai-hub token::tests::token_scope_guard` (name to be confirmed) to verify the guard.

## Risks
- If an empty `endpoint_scopes` list is interpreted differently by other layers, callers should double-check their expectations before calling the guard.
