## 1. Database-backed bootstrap

- [x] 1.1 Add typed web-server database configuration and bootstrap errors for a file-backed SQLite path.
- [x] 1.2 Replace the in-memory `BootstrapProjectApi` composition in `apps/web/server/src/bootstrap.rs` with `ora-db` bootstrap, repository-pool construction, and handler wiring for `project`, `task`, `worktree`, and `session`.
- [x] 1.3 Ensure readiness is only marked after the SQLite-backed application state has initialized successfully.

## 2. Shared runtime state and HTTP handlers

- [x] 2.1 Expand `AppState` to expose the transport-facing CRUD entry points for all four entity families without leaking repository construction into routes.
- [x] 2.2 Add `task` HTTP handlers and request adapter structs that delegate to the existing `ora-application` task handlers.
- [x] 2.3 Add `worktree` HTTP handlers and request adapter structs that delegate to the existing `ora-application` worktree handlers.
- [x] 2.4 Add `session` HTTP handlers and request adapter structs that delegate to the existing `ora-application` session handlers.

## 3. Routes and transport behavior

- [x] 3.1 Register the new `/api/tasks`, `/api/worktrees`, and `/api/sessions` CRUD routes alongside the existing project routes.
- [x] 3.2 Extend centralized HTTP error handling so not-found and repository failures remain stable across project, task, worktree, and session routes.
- [x] 3.3 Remove or rename bootstrap-only adapter types and comments so the runtime no longer describes itself as an in-memory project-only server.

## 4. Verification and documentation

- [x] 4.1 Add or update route tests to cover SQLite-backed project persistence plus representative CRUD flows for task, worktree, and session.
- [x] 4.2 Update bootstrap/configuration tests for the new database-path runtime contract and failure cases.
- [x] 4.3 Refresh `docs/web-server-runtime.md` to document the persisted SQLite-backed runtime and the expanded HTTP API surface.
