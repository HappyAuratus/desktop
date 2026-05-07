## Why

The project has a concrete SQLite schema in `docs/schema.sql`, but the Rust domain layer does not yet expose first-class models that represent those records and their categorical fields. Implementing the domain models now creates a single source of truth for business entities before transport and persistence layers grow around ad hoc structs and raw integers.

## What Changes

- Add Rust domain models for `projects`, `tasks`, `worktrees`, `virtual_folders`, `virtual_entries`, `sessions`, and `artifacts` based on `docs/schema.sql`.
- Model numeric category columns such as task status, virtual entry kind, session status, and worktree active state as Rust enums instead of raw integers.
- Define conversion boundaries so persistence-oriented numeric representations stay outside the domain layer.
- Organize the domain crate exports so other layers can depend on stable, strongly typed entities and value types.

## Capabilities

### New Capabilities
- `domain-models`: Define strongly typed Rust domain entities and enums derived from the SQLite schema so higher layers share one canonical model.

### Modified Capabilities

## Impact

- Affected code: Rust workspace under `crates/`, especially the domain crate and any shared model exports.
- Affected APIs: Internal Rust APIs that currently pass raw integers or loosely typed record structs for schema-backed entities.
- Dependencies: No new external services are required; serde and existing Rust workspace dependencies may be reused for model serialization if needed.
- Systems: Desktop Tauri and web server code will both consume the same domain entities once implemented.
