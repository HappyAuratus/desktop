## Context

`apps/web/server` already has a modular runtime shell, but its composition root still stops at `BootstrapProjectApi`, which is backed by a process-local in-memory repository. At the same time, the workspace already contains the pieces needed for a persistent multi-entity backend: `ora-db` can bootstrap a file-backed SQLite database and expose pooled repository adapters, `ora-application` already defines CRUD handlers for `project`, `task`, `worktree`, and `session`, and `ora-contracts` already provides the transport DTOs for those slices.

The remaining gap is integration inside the web adapter. The server needs to bootstrap a real on-disk database, compose all four handler families into shared state, expose the missing HTTP routes, and keep startup, readiness, and transport error behavior coherent once persistence is no longer ephemeral.

## Goals / Non-Goals

**Goals:**

- Replace the in-memory bootstrap repository wiring in `apps/web/server` with a file-backed SQLite composition that uses `ora-db` bootstrap and repository pool APIs.
- Extend the shared web runtime state so route handlers can delegate into `project`, `task`, `worktree`, and `session` application handlers.
- Add HTTP CRUD handlers and routes for `task`, `worktree`, and `session` using existing `ora-contracts` DTOs and the same adapter conventions already used for `project`.
- Make database bootstrap failures part of the typed web-server startup surface and keep readiness coupled to successful runtime initialization.
- Update runtime documentation and adapter tests so the server contract reflects the persisted multi-model backend.

**Non-Goals:**

- Changing `ora-domain`, `ora-contracts`, `ora-application`, or `ora-db` business behavior beyond the integration points the web server needs.
- Adding new database schema migrations or changing repository semantics in `ora-db`.
- Introducing filtered or relationship-aware endpoints beyond the existing CRUD contracts for the four entity families.
- Designing auth, background jobs, or production deployment concerns in this change.

## Decisions

### Bootstrap the web server through `ora-db` using a file-backed database path

The web server will stop constructing an adapter-local repository implementation in `bootstrap.rs` and will instead call `DatabaseBootstrapper::bootstrap_repository_pool` with a file-backed `DatabaseLocation`. The runtime configuration will grow a typed database-path setting, exposed through a dedicated environment variable, and startup will fail early if the database cannot be opened, migrated, or pooled.

Why:
- This reuses the persistence path the workspace already standardized in `ora-db` instead of keeping a second bootstrap-only storage implementation alive inside the adapter.
- It makes the running HTTP server behave like the persisted product model the rest of the stack expects.

Alternative considered:
- Keep the in-memory repository for `project` and add separate SQLite wiring only for the new entities.
  Rejected because it would split persistence semantics inside one runtime and make cross-entity behavior harder to reason about and test.

### Replace `BootstrapProjectApi` with one shared multi-model application state

`AppState` will evolve from a thin wrapper around one project-only API aggregate into a shared runtime object that owns the CRUD entry points for `project`, `task`, `worktree`, and `session`. The composition root may still use small adapter-focused aggregate structs per entity family, but those aggregates will be backed by the same repository pool and clock source so route handlers consume one coherent runtime state value.

Why:
- The adapter already follows a pattern where routes delegate into transport-facing aggregates; extending that pattern preserves consistency without pushing orchestration into route functions.
- Shared state keeps readiness, cloning behavior, and test setup simpler than letting each route tree bootstrap its own dependencies.

Alternative considered:
- Store raw repositories in `AppState` and let HTTP handlers instantiate application handlers per request.
  Rejected because it would duplicate composition logic across handlers and weaken the adapter boundary that already exists.

### Keep one handler module per entity and mirror route conventions across all CRUD slices

The HTTP adapter will add `tasks.rs`, `worktrees.rs`, and `sessions.rs` handler modules under `apps/web/server/src/handlers/`, with route shapes parallel to the existing project slice:

- `/api/projects`
- `/api/tasks`
- `/api/worktrees`
- `/api/sessions`

Each slice will expose create, get, list, update, and delete operations, will deserialize directly into `ora-contracts` request DTOs or small path/body adapter structs, and will map results through the shared `WebApiError`.

Why:
- Parallel route and handler shapes make the adapter easy to scan and extend.
- The contracts already exist, so the lowest-risk integration is to mirror the proven project HTTP flow for the remaining entities.

Alternative considered:
- Create one generic CRUD HTTP module shared by every entity family.
  Rejected because the request bodies, identifiers, and response wrappers still differ enough that a generic abstraction would add indirection without much code reduction.

### Centralize database-aware startup and error translation in the web adapter

The web runtime will treat database path parsing, database bootstrap, and repository pool construction as first-class bootstrap concerns. `WebBootstrapError` will expand to include database configuration and bootstrap failures, while `WebApiError` will continue to translate application-layer not-found and repository errors into stable HTTP responses across all four entity families.

Why:
- Database-backed startup introduces failure modes that should be explicit and testable, not hidden behind generic I/O errors.
- The adapter already has a centralized error mapping pattern; extending it is safer than adding per-route special cases.

Alternative considered:
- Let database initialization panic or bubble as opaque boxed errors.
  Rejected because it would make startup failures harder to diagnose and inconsistent with the rest of the typed bootstrap surface.

### Update the runtime contract and tests around persisted behavior

The docs and route tests will stop describing the web server as an in-memory bootstrap runtime. Tests will cover successful database-backed CRUD behavior and representative not-found/error cases for the newly exposed route families, ideally by bootstrapping isolated file-backed SQLite databases per test so the adapter exercises the same path it will use in production.

Why:
- The current runtime documentation explicitly advertises in-memory storage; that becomes wrong as soon as this change lands.
- Using file-backed test databases catches integration problems that in-memory fakes would miss, especially around `RepositoryPool` and migration bootstrap.

Alternative considered:
- Keep the existing route tests and rely on lower-layer `ora-db` tests for persistence confidence.
  Rejected because this change is specifically about adapter integration, and the adapter needs its own end-to-end verification.

## Risks / Trade-offs

- [Adding a required database path changes web-server startup expectations] -> Mitigation: provide a documented default path and surface invalid or unusable paths through typed bootstrap errors.
- [A multi-model `AppState` can become a grab bag if responsibilities blur] -> Mitigation: keep entity-specific aggregates narrow and expose only the handler families routes actually need.
- [Route coverage for four CRUD families increases adapter test volume] -> Mitigation: share test helpers for bootstrapped routers and favor a few representative end-to-end flows plus targeted error-path tests.
- [File-backed SQLite tests can be slower or more brittle than in-memory tests] -> Mitigation: use temp directories and keep tests focused on integration seams that truly require the pooled file-backed path.

## Migration Plan

1. Add database path configuration and typed bootstrap errors to `apps/web/server`.
2. Replace `BootstrapProjectApi` and the in-memory repository implementation in `bootstrap.rs` with `ora-db` repository-pool wiring for all four entity families.
3. Expand `AppState`, add the missing handler modules, and register the task/worktree/session routes.
4. Update adapter tests to exercise the SQLite-backed runtime path and add coverage for the new routes.
5. Refresh `docs/web-server-runtime.md` so the documented runtime contract matches the persisted backend behavior.

Rollback strategy:
- Revert the web-server integration change as one unit and restore the in-memory bootstrap composition if the SQLite-backed runtime proves unstable. No schema rollback is required because this change only consumes the existing `ora-db` bootstrap and migration catalog.

## Open Questions

- None at this stage.
