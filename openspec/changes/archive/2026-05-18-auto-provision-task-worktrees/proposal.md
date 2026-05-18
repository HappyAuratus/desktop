## Why

Task creation currently treats tasks and worktrees as separate CRUD flows, which leaves callers responsible for inventing and wiring a `worktree_id` even though every task is expected to operate inside its own Git worktree. We need task creation to own that provisioning path now that the runtime already has linked-worktree Git support, so new tasks start from a consistent, server-managed workspace invariant.

## What Changes

- **BREAKING** Remove caller-supplied `worktree_id` from task creation contracts so the server always provisions and assigns the task worktree internally.
- Make task creation orchestrate linked Git worktree provisioning, worktree persistence, and task persistence as one backend flow.
- Make task deletion remove the task-owned linked Git worktree and use force mode for the first version.
- Add typed web runtime configuration for the worktree root directory used to place per-task linked worktrees under the configured project checkout.
- Update task creation error handling so infrastructure failures from Git worktree provisioning surface as stable application and HTTP failures.

## Capabilities

### New Capabilities
- `task-worktree-provisioning`: Define the invariant that creating a task provisions exactly one linked Git worktree and persists the resulting task-worktree relationship.

### Modified Capabilities
- `app-contracts`: Change task create request and response expectations so callers no longer provide `worktree_id` during task creation.
- `application-handlers`: Change task handler behavior from passive CRUD persistence to task-owned worktree provisioning orchestration.
- `web-server-runtime`: Change bootstrap configuration and HTTP task-create runtime behavior to provide a configured worktree root and surface provisioning failures consistently.

## Impact

- Affected code includes `ora-contracts`, `ora-application`, `ora-db`, `ora-gitlancer`, and `apps/web/server`.
- Task creation APIs and generated frontend types will change because `CreateTaskRequest.worktree_id` is removed.
- The web runtime gains a required `ORA_WORK_DIR` configuration input and a new dependency that provisions linked worktrees from the configured project repository.
