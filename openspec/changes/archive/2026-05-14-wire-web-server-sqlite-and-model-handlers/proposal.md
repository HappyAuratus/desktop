## Why

`apps/web/server` currently serves only the `project` HTTP slice and still bootstraps that slice through an in-memory `BootstrapProjectApi`. That leaves the server out of sync with the rest of the workspace: `ora-db` already provides file-backed SQLite repositories, and `ora-application` / `ora-contracts` already define CRUD flows for `task`, `worktree`, and `session`, but none of those capabilities are reachable through the running backend.

## What Changes

- Replace the bootstrap-only in-memory project repository wiring in `apps/web/server/src/bootstrap.rs` with a real file-backed SQLite composition rooted in `ora-db`.
- Extend the web server application state so HTTP handlers can delegate into `project`, `task`, `worktree`, and `session` application handlers from one shared runtime bootstrap.
- Add HTTP handlers and routes for the remaining CRUD slices backed by existing `ora-contracts` request and response DTOs:
  - `task`
  - `worktree`
  - `session`
- Update bootstrap configuration and error handling as needed so database-backed startup failures surface clearly and readiness reflects a successfully initialized runtime.
- Add route and integration-style tests that verify the server can exercise the SQLite-backed project flow and the newly exposed task/worktree/session CRUD endpoints.

## Capabilities

### New Capabilities
- `web-server-runtime`: Define the database-backed web server composition root, shared runtime state, and HTTP CRUD route surface for `project`, `task`, `worktree`, and `session`.

### Modified Capabilities

## Impact

- Affected code: `apps/web/server`, especially `bootstrap.rs`, `app_state.rs`, `routes.rs`, `handlers/`, `config.rs`, and the adapter test surface.
- Affected APIs: the HTTP backend will expose new CRUD endpoints for `task`, `worktree`, and `session`, and existing project routes will persist through a disk-backed SQLite database instead of process-local memory.
- Dependencies and systems: the server bootstrap will compose `ora-db` repository pools and database bootstrap/migration flows during startup, making local runtime behavior match the persisted workspace model more closely.
