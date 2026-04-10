# Entity, DTO, VO, And Schema Sync Patterns

This reference focuses on the workspace's current data-model conventions:
SeaORM 2.0, entity-first modeling, and no database foreign keys.

## Canonical Examples

- System entity extension entry: `crates/summer-system/model/src/entity/sys_user.rs`
- System DTO: `crates/summer-system/model/src/dto/sys_user.rs`
- System VO: `crates/summer-system/model/src/vo/sys_user.rs`
- Shared schema sync plugin: `crates/summer-plugins/src/entity_schema_sync.rs`

## Model Crate Split

- `crates/summer-system/model`: system entities, DTOs, VOs, and `views`
- `crates/summer-ai/model`: AI domain entities, DTOs, and VOs
- Future business model crates can reuse the same pattern, but do not cite
  non-existent crates as if they already exist

## `src/entity` Convention

- `src/entity` is the stable entity layer that application code should depend on
- Put generated entity definitions and `ActiveModelBehavior` extensions together
  in the same module file

### Import Rule

Business code should use:

- `summer_system_model::entity::sys_user`

## SeaORM 2.0 Entity-First Rules

### Relation Rule

This workspace does not rely on database foreign keys.

That means:

- `has_many` and `via` relations are fine for navigation
- `belongs_to` is still useful, but usually paired with `skip_fk`

Example:

```rust
#[sea_orm(belongs_to, from = "user_id", to = "id", skip_fk)]
pub user: Option<super::sys_user::Entity>,
```

This preserves SeaORM relation and join ergonomics without forcing database
foreign keys into the schema.

### Naming And Renames

- Use `column_name = "..."` when only the Rust field name changes
- Use `renamed_from = "..."` when the database column is actually being renamed

Do not silently rename fields and expect schema sync to infer intent.

## What Schema Sync Can Do

In this repo, schema sync is best understood as "safe structure filling", not a
full diff engine.

Usually safe:

- create new tables
- add new columns
- rename columns when `renamed_from` is explicit
- add normal indexes
- add unique indexes / composite unique keys

Do not rely on it for:

- dropping tables
- dropping columns
- changing column types
- changing nullability
- changing defaults
- syncing comments

Those changes should go through explicit SQL or migrations.

## Entity Pattern

Common traits and patterns in this repo:

- `DeriveEntityModel`
- `DeriveActiveEnum` for business enums
- `Serialize` / `Deserialize`
- `JsonSchema` where API contracts need it
- timestamps and write-time defaults handled in the stable entity layer

## DTO Pattern

### Create DTO

Create DTOs should own:

- validation
- defaulting
- conversion into `ActiveModel`

### Update DTO

Update DTOs should:

- only expose mutable fields
- implement `apply_to()` against an `ActiveModel`

### Query DTO

Prefer query DTOs that convert to `Condition` so services can write:

```rust
sys_user::Entity::find().filter(query)
```

## VO Pattern

VOs are frontend contracts, not entity mirrors.

Common patterns:

- `#[serde(rename_all = "camelCase")]`
- `from_model()` conversion helpers
- enum values converted into frontend-friendly output when needed

## Minimal Flow For A New Entity

1. Generate or update the entity definitions directly in `entity`
2. Add stable behavior in `entity`
3. Add DTOs for input and validation
4. Add VOs for output contracts
5. Add query DTOs for filtering
6. If schema sync is involved, make sure the change belongs to the "safe
   structure filling" category

## Anti-Patterns

- Do not add database foreign keys just because SeaORM supports relations
- Do not push frontend-only fields into entities
- Do not let routers mutate `ActiveModel`
- Do not assume schema sync handles dangerous schema changes
