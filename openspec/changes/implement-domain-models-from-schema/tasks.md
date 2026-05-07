## 1. Domain crate structure

- [x] 1.1 Create or update the Rust domain crate module tree for schema-backed entities under `crates/`.
- [x] 1.2 Add explicit public exports for the new domain entities and enum types while keeping internal modules private by default.

## 2. Schema-backed entities and enums

- [x] 2.1 Implement Rust models for projects, tasks, worktrees, virtual folders, virtual entries, sessions, and artifacts based on `docs/schema.sql`.
- [x] 2.2 Replace raw numeric category fields with Rust enums for task status, worktree active state, virtual entry kind, and session status.
- [x] 2.3 Preserve schema nullability in the domain models by using optional fields only for nullable columns.

## 3. Conversion boundaries and validation

- [x] 3.1 Add explicit conversion helpers or mapper-facing APIs that translate persisted numeric values into domain enums.
- [x] 3.2 Reject unsupported numeric category values during conversion instead of allowing invalid domain states.

## 4. Tests and documentation

- [x] 4.1 Add unit tests covering enum conversions, invalid value rejection, and representative entity construction.
- [x] 4.2 Update relevant `docs/` architecture or modeling documentation if the new domain crate API changes how shared models are consumed.
- [ ] 4.3 Run the Rust test suite for the affected crates and confirm the new domain models compile cleanly for both desktop and web consumers.
