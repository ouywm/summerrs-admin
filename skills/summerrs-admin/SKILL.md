---
name: summerrs-admin
description: >
  Use when implementing or modifying code in the Summerrs Admin workspace and
  repo conventions determine crate placement, route/service layering, SeaORM
  entity organization, auth or Socket.IO integration, plugin wiring, or MCP
  generator usage.
---

# Summerrs Admin

This skill captures repository-specific engineering patterns for the Summerrs Admin
workspace. Use it when the answer depends on how this repo is structured, not when
the task is generic Rust work.

## When To Use

- You need to decide whether code belongs in `crates/app`, `crates/summer-system`,
  `crates/summer-plugins`, `crates/summer-mcp`, `crates/summer-rig`, or
  `crates/summer-ai/*`.
- You are adding or modifying routes, services, DTOs, VOs, plugins, Socket.IO
  handlers, or SeaORM entities.
- You are generating code with MCP and need to move or adapt the output to the
  repo's actual layout.
- You are touching auth flows, online users, session revocation, or realtime
  behavior.

## When Not To Use

- The task is generic Rust, Cargo, SQL, or Git work and does not depend on this
  workspace's conventions.
- You only need a single crate's local implementation details and the relevant
  files are already open.

## Task Routing

- Route/service/DTO/VO work: read `references/route-service.md`
- Plugin, component, config, and app wiring: read `references/plugin-component.md`
- SeaORM entities, DTO/VO contracts, and schema sync: read
  `references/data-model.md`
- MCP generators and codegen landing zones: read `references/mcp-generator.md`
- Auth, online users, kick-out flows, and Socket.IO: read
  `references/auth-realtime.md`

## Hard Constraints

- Keep routers thin and services thick.
- `crates/app/src/main.rs` is the assembly root. Do not treat the entire
  `crates/app` crate as a business-logic crate.
- System business code belongs in `crates/summer-system`.
- System DTO/VO/entity code belongs in `crates/summer-system-model`.
- AI model contracts live under `crates/summer-ai/model`; AI runtime and relay
  behavior live under `crates/summer-ai/hub`.
- Shared infrastructure plugins belong in `crates/summer-plugins`.
- Generated raw entities belong in `src/entity_gen`; stable application code
  should depend on `src/entity`.
- Do not rely on database foreign keys. Use SeaORM relations with `skip_fk`
  where appropriate.
- Prefer MCP business tools for menus and dictionaries instead of hand-written
  SQL.

## Preferred Patterns

- Prefer `summer_common::response::Json<T>` for route responses.
- Put `#[log]` above the route macro.
- Use services for transactions, aggregation, and policy logic.
- Use query DTOs that can be passed directly into `.filter(query)` where possible.
- If a plugin depends on another plugin's components, declare `dependencies()`.
- If MCP generators emit code into legacy paths, move the generated output into
  the current repo layout before integrating it.

## Key Crates And Anchor Files

- App assembly root: `crates/app/src/main.rs`
- System route examples: `crates/summer-system/src/router/sys_user.rs`,
  `crates/summer-system/src/router/auth.rs`
- System service examples:
  `crates/summer-system/src/service/sys_user_service.rs`,
  `crates/summer-system/src/service/online_service.rs`
- System Socket.IO entry points:
  `crates/summer-system/src/plugins/socket_gateway.rs`,
  `crates/summer-system/src/socketio/connection/*`,
  `crates/summer-system/src/socketio/core/*`
- System model layer:
  `crates/summer-system-model/src/entity/*`,
  `crates/summer-system-model/src/entity_gen/*`,
  `crates/summer-system-model/src/dto/*`,
  `crates/summer-system-model/src/vo/*`,
  `crates/summer-system-model/src/views/*`
- Shared schema sync plugin:
  `crates/summer-plugins/src/entity_schema_sync.rs`
- MCP entry points:
  `crates/summer-mcp/src/plugin.rs`,
  `crates/summer-mcp/src/runtime.rs`,
  `crates/summer-mcp/src/server.rs`,
  `crates/summer-mcp/src/table_tools/router.rs`,
  `crates/summer-mcp/src/tools/*`
- AI runtime:
  `crates/summer-ai/hub/src/plugin.rs`,
  `crates/summer-ai/hub/src/router/*`,
  `crates/summer-ai/hub/src/service/*`
- Rig plugin:
  `crates/summer-rig/src/plugin.rs`

## Verification

- Before finishing, run `build-tools/pre-commit` or the relevant formatter/check
  commands for the crates you touched.
- If you changed routes, services, auth, or Socket.IO behavior, do at least one
  targeted runtime sanity check in addition to compile/test checks.

## Local References

The `references/*.md` files in this skill directory contain the repo-specific
details. Use them as the detailed guidance layer; keep this top-level skill short
and decision-oriented.
