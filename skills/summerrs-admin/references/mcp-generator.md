# MCP And Generator Patterns

This reference explains how MCP is used in this repo: discover schema first,
generate raw code second, then move or adapt the output into the workspace's
current layout.

## Canonical Examples

- MCP plugin: `crates/summer-mcp/src/plugin.rs`
- Runtime and server: `crates/summer-mcp/src/runtime.rs`,
  `crates/summer-mcp/src/server.rs`
- Table-tools router: `crates/summer-mcp/src/table_tools/router.rs`
- Generator implementations: `crates/summer-mcp/src/tools/*`

## When MCP Should Be First

Prefer MCP before handwritten code when:

- a table already exists and you need raw entities
- you want a fast CRUD skeleton
- you need generated frontend API / types / page bundles
- you need menu or dictionary plans

## Recommended Order

1. `schema_list_tables` or `schema://tables`
2. `schema_describe_table` or `schema://table/{table}`
3. `generate_entity_from_table`
4. `generate_admin_module_from_table`
5. `generate_frontend_bundle_from_table`
6. `menu_tool` / `dict_tool`

Do not skip schema discovery and guess field layouts.

## Key MCP Capability Groups

### Schema Discovery

- `schema_list_tables`
- `schema_describe_table`
- `schema://tables`
- `schema://table/{table}`

### Generic Table CRUD

- `table_get`
- `table_query`
- `table_insert`
- `table_update`
- `table_delete`

### SQL Escape Hatches

- `sql_query_readonly`
- `sql_exec`

### Generators

- `generate_entity_from_table`
- `upgrade_entity_enums_from_table`
- `generate_admin_module_from_table`
- `generate_frontend_api_from_table`
- `generate_frontend_page_from_table`
- `generate_frontend_bundle_from_table`

### Business Tools

- `menu_tool`
- `dict_tool`

## Target Landing Zones In This Repo

For system code, the intended repo-native landing zones are:

- stable entities: `crates/summer-system/model/src/entity`
- DTOs: `crates/summer-system/model/src/dto`
- VOs: `crates/summer-system/model/src/vo`
- routes: `crates/summer-system/src/router`
- services: `crates/summer-system/src/service`
- frontend route modules: `crates/summer-system/frontend-routes`

## Important Warning About Generator Defaults

The current MCP generators still carry some legacy default output paths in code.
Do not assume the generated files automatically land in the live crate layout.

Before integrating generator output, verify whether it targeted legacy paths such as:

- `crates/app/src/router`
- `crates/app/src/service`
- `crates/model/src/*`
- `crates/app/frontend-routes`

If that happens, move and adapt the generated output into the current layout under
`summer-system`, `crates/summer-system/model`, and `crates/summer-system/frontend-routes`.

## Backend CRUD Generation

For the common flow "existing table -> admin module":

1. inspect the schema
2. generate the raw entity
3. generate the admin module skeleton
4. move files if the generator used legacy defaults
5. add business semantics, permissions, logs, menu entries, and dictionaries

Do not expect generators to solve:

- login and auth flows
- multi-table aggregation endpoints
- online-user or kick-out flows
- Socket.IO behavior
- complex state machines

## Menus And Dictionaries

### Menus

Prefer MCP business tools over handwritten SQL:

- `menu_tool.plan_config`
- `menu_tool.export_config`
- `menu_tool.apply_config`

### Dictionaries

Prefer MCP business tools over handwritten SQL:

- `dict_tool.plan_bundle`
- `dict_tool.export_bundle`
- `dict_tool.apply_bundle`

## Verification

- MCP changes: `cargo test -p summer-mcp`
- Larger generator changes: run the relevant workspace checks for the affected
  crates and verify generated output paths before committing
