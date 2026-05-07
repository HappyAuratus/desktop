## Context

`docs/schema.sql` defines the persistence shape for Ora's project, task, worktree, virtual file system, session, and artifact data, but the Rust workspace does not yet provide a dedicated domain crate API that mirrors those records. Without a shared domain model, transport and persistence code will tend to invent local structs and pass raw numeric category values, which makes the model harder to evolve consistently across the desktop and web applications.

The schema also uses integer columns for categorical state:

- `tasks.status`
- `worktrees.is_active`
- `virtual_entries.kind`
- `sessions.status`

Those columns are compact for SQLite, but they are too weak for the domain layer because invalid numeric states remain representable until runtime. The domain model needs to make those states explicit and exhaustively matchable in Rust.

## Goals / Non-Goals

**Goals:**

- Introduce first-class Rust domain entities for every table defined in `docs/schema.sql`.
- Represent numeric category columns as Rust enums or enum-backed value types inside the domain layer.
- Keep persistence-specific integer encoding at the repository or mapper boundary instead of leaking it into callers.
- Expose a domain API that both Tauri and HTTP paths can depend on without transport-specific knowledge.

**Non-Goals:**

- Implement repositories, SQL query code, or database migrations in this change.
- Redesign the schema itself or introduce foreign keys that are intentionally absent today.
- Define HTTP or Tauri transport DTOs beyond what is needed to consume the new domain types later.

## Decisions

### Create a dedicated domain model module tree keyed by schema entities

The implementation will add one focused module per schema-backed aggregate or entity, plus small supporting modules for enums and shared identifiers where needed. This keeps each model file small enough to evolve independently and aligns with the existing Rust guidance to prefer smaller modules and explicit public exports.

Alternative considered:

- One large `models.rs` file containing every struct and enum. Rejected because the schema already spans multiple related concerns and a single file would become difficult to navigate as invariants and constructors grow.

### Model categorical integers as exhaustive Rust enums

`TaskStatus`, `WorktreeActivity`, `VirtualEntryKind`, and `SessionStatus` will be represented as enums in the domain crate. If conversion from persisted integers is required, it will happen through explicit conversion helpers that can reject unknown values rather than silently accepting arbitrary integers.

This choice follows the repository guidance to make illegal states unrepresentable and gives higher layers exhaustive `match` behavior.

Alternatives considered:

- Keep raw `i64` or `u8` fields in the domain structs. Rejected because it preserves invalid states and obscures meaning at call sites.
- Use wrapper newtypes over integers without enums. Rejected because callers would still need out-of-band knowledge of the allowed values and would lose exhaustiveness.

### Keep table rows as simple entities and reserve mapping logic for adapters

The initial domain structs will stay close to the schema columns so the system has a canonical representation of stored records. Any SQLite row decoding, serde flattening, or transport reshaping belongs in mapper code outside the domain crate.

Alternative considered:

- Put `sqlx` or SQLite conversion derives directly on domain models. Rejected because it would couple the domain to storage technology and make future testing or alternate persistence harder.

### Use explicit optionality only where the schema allows null

Fields such as `worktree_id`, `branch_name`, `parent_entry_id`, and `content_ref` will remain optional in the domain because the schema allows them to be absent. Required columns will stay non-optional so the type system continues reflecting persistence constraints.

Alternative considered:

- Normalize all nullable fields into empty strings or sentinel IDs. Rejected because it hides absence semantics and makes invalid states easier to construct.

## Risks / Trade-offs

- [Risk] Existing or future persistence code may still want raw integers for storage. -> Mitigation: add narrow conversion helpers at the boundary and keep integer encoding out of the public domain struct fields.
- [Risk] The schema omits foreign keys, so domain entities cannot encode referential integrity by construction. -> Mitigation: keep relational identifiers typed and let application services validate cross-entity references when orchestration is added.
- [Risk] Modeling `is_active` as a richer enum may feel heavier than a boolean. -> Mitigation: use a small enum with semantic names so call sites stay self-documenting and future additional states remain representable without boolean creep.

## Migration Plan

1. Create the domain crate modules and export surface for schema-backed entities.
2. Introduce enums for each numeric category field and wire them into the owning structs.
3. Add conversion tests that prove valid numeric mappings succeed and unknown values fail.
4. Update downstream crates to consume the new domain types once implementation begins.

Rollback is low risk because this change only adds the modeling layer. If the direction proves incorrect, the new domain modules can be revised before any persistence or transport code adopts them broadly.

## Open Questions

- Should identifier columns remain plain `String` in the first pass or move immediately to typed ID newtypes for stronger domain guarantees?
A: typed ID newtypes
- Should `is_active` be modeled as a generic boolean-style enum (`Active`/`Inactive`) or a worktree-specific enum name to leave room for future lifecycle states?
A: just boolean
