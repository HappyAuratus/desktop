## Context

Task creation currently persists a `Task` directly from `CreateTaskRequest` and accepts an optional caller-supplied `worktree_id`. That shape no longer matches the intended invariant from the product discussion in [BRAINSTORM.md](/root/projects/desktop/docs/BRAINSTORM.md): every task should own one internal linked Git worktree created by the backend. The repository already has the building blocks for this in `ora-gitlancer`, but the web runtime does not yet provide a configured worktree root or a task-specific provisioning dependency.

This change crosses multiple layers. `ora-contracts` must stop exposing worktree selection during task creation, `ora-application` must orchestrate Git worktree creation and persistence without becoming Git-aware, and `apps/web/server` must thread a configured project repository root plus worktree root into the runtime bootstrap.

## Goals / Non-Goals

**Goals:**
- Make task creation provision exactly one linked Git worktree automatically.
- Preserve a transport-agnostic application layer by introducing a task worktree provisioning port instead of calling Git APIs directly from handlers.
- Configure the linked-worktree root from the web runtime so worktree directories are deterministic and colocated with the configured project checkout.
- Keep task reads and updates compatible with the invariant that created tasks always have an assigned `worktree_id`.

**Non-Goals:**
- Changing standalone worktree CRUD behavior outside the task-create flow.
- Supporting caller-selected branches, alternate start points, or custom worktree locations.
- Exposing internal task worktrees as a user-selectable model in public APIs.

## Decisions

### Introduce a task-owned worktree provisioning port in `ora-application`

`CreateTaskHandler` will gain a new dependency trait responsible for provisioning and, if needed, deleting a task worktree. The handler will remain responsible for orchestration order, domain identifiers, and persistence, while the runtime-owned implementation will translate those requests into `ora-gitlancer` calls.

Why this design:
- It keeps Git side effects outside the task repository abstraction.
- It preserves unit-testability with in-memory fakes for both repositories and provisioning.
- It follows the repository's existing handler-owned dependency pattern.

Alternatives considered:
- Calling `ora-gitlancer` directly from the handler. Rejected because it would couple the application crate to infrastructure and complicate tests.
- Hiding worktree provisioning inside `TaskRepository::create_task`. Rejected because task persistence and Git mutation have different failure modes and should remain explicit.

### Make task IDs drive both branch names and filesystem paths

The handler will generate the task ID first, then derive:
- branch name: `ora/<first-8-uuid-characters>`
- worktree path: `<ORA_WORK_DIR>/<full-task-id>`

Why this design:
- The branch name stays short enough for routine Git use while still being task-derived.
- Fixing the prefix at eight UUID characters keeps the naming rule simple, explicit, and stable across implementations.
- The directory path uses the full task ID to avoid collisions on disk.
- The derivation is deterministic, so failures and cleanup can reconstruct the target worktree location from the task ID alone.

Alternatives considered:
- Using the full task ID in branch names. Rejected because it makes Git branch references unnecessarily long.
- Reusing an implementation-local short-ID helper. Rejected because this contract should not depend on whether a helper happens to exist in the codebase during implementation.
- Using task titles in branch or directory names. Rejected because titles are mutable and can contain filesystem-hostile characters.

### Provision the Git worktree before persisting the worktree and task rows

The create flow will run in this order:
1. Generate the task ID.
2. Derive branch name and worktree root.
3. Provision the linked Git worktree.
4. Persist the `worktree` row with a newly generated `worktree_id`.
5. Persist the `task` row with that `worktree_id`.

If step 4 or 5 fails after Git provisioning succeeds, the handler will issue a compensating delete through the provisioning port before returning an application error.

Why this design:
- It avoids persisting a task or worktree row that points to a Git worktree that was never created.
- Compensation keeps the filesystem and database aligned when persistence fails late.

Alternatives considered:
- Persisting DB rows first, then creating the Git worktree. Rejected because Git failure would leave visible rows in an unusable state.
- Accepting orphaned Git worktrees on DB failure. Rejected because cleanup would become manual and operationally noisy.

### Put `ORA_WORK_DIR` in the web runtime `ProjectConfig`

The configured worktree root belongs with the configured project checkout because the runtime must treat them as one workspace definition. `ProjectConfig` will therefore load `ORA_WORK_DIR` alongside `ORA_PROJECT_NAME` and `ORA_PROJECT_PATH`, and bootstrap will pass that into the task-provisioning implementation.

Why this design:
- The worktree root is conceptually part of project workspace bootstrap, not a generic database or server concern.
- It keeps all project-root-dependent runtime configuration in one typed config object.

Alternatives considered:
- A top-level runtime field separate from project config. Rejected because it splits one workspace concept across unrelated config structs.
- Deriving the worktree root implicitly from `ORA_PROJECT_PATH`. Rejected because the brainstorm explicitly calls for a dedicated internal worktree root and different deployments may want a separate directory.

### Remove the linked worktree during task deletion and use force mode in the first version

`DeleteTaskHandler` will expand from pure row deletion into task-owned workspace cleanup. Before finalizing task deletion, the backend will resolve the task's linked worktree and remove it through the provisioning port using force mode, then soft-delete the task record. The first version will prefer reliable cleanup over preserving uncommitted filesystem state.

Why this design:
- The task-to-worktree relationship is intended to be owned by the backend, so deletion should clean up the owned workspace as well.
- Force mode avoids deletion failures caused by dirty worktrees or Git protection prompts, which is a better fit for an internal, non-user-managed checkout.

Alternatives considered:
- Leaving linked worktrees on disk after task deletion. Rejected because it would leak internal workspaces and break the ownership invariant.
- Using checked deletion mode first. Rejected for now because dirty internal worktrees would make ordinary task deletion unreliable.

## Risks / Trade-offs

- [Persistence fails after Git worktree creation] → Add compensating delete logic in the handler and cover it with unit tests.
- [Compensating delete also fails] → Return a stable repository/provisioning failure while logging the cleanup failure context for manual intervention.
- [Forced task deletion removes uncommitted worktree changes] → Accept this trade-off for the first version because the linked worktree is an internal backend-owned workspace, and document the behavior clearly.
- [Branch prefix collisions across many tasks] → Use only the short prefix for the branch name but keep the full task ID in the directory path; if collisions become likely later, widen the prefix without changing the filesystem contract.
- [Runtime misconfiguration causes task creation failures] → Validate `ORA_WORK_DIR` at startup the same way the runtime already validates project and database configuration.

## Migration Plan

1. Add `ORA_WORK_DIR` to runtime configuration, bootstrap validation, and environment documentation.
2. Update `CreateTaskRequest` and generated frontend metadata to remove caller-supplied `worktree_id`.
3. Introduce the application-layer provisioning port plus compensation behavior in `CreateTaskHandler`.
4. Implement the port in the web runtime using `ora-gitlancer` and the configured project/worktree roots.
5. Update route, handler, and integration tests to assert that creating a task yields a persisted task and worktree pair, and that deleting a task removes its linked worktree with force mode.

Rollback strategy:
- Revert the runtime wiring, contract change, and handler orchestration together. Because this is a breaking API change, partial rollback is not safe.

## Open Questions
