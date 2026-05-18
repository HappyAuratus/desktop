## 1. Contracts and configuration

- [x] 1.1 Remove `worktree_id` from `CreateTaskRequest`, regenerate any frontend/export metadata that depends on task create contracts, and update contract tests for the new request shape.
- [x] 1.2 Extend the web runtime configuration to require `ORA_WORK_DIR` as part of `ProjectConfig`, including validation, bootstrap error coverage, and any relevant `docs/` updates for the new environment variable.

## 2. Task worktree provisioning flow

- [x] 2.1 Add an application-layer task worktree provisioning port and supporting error mapping that let `CreateTaskHandler` provision and clean up linked worktrees without depending on concrete Git types.
- [x] 2.2 Refactor `CreateTaskHandler` to generate the task ID first, derive the branch name and worktree path, provision the linked worktree, persist the worktree row, and persist the task row with the assigned `worktree_id`.
- [x] 2.3 Implement compensating cleanup so persistence failures after Git provisioning attempt to remove the created linked worktree before the handler returns an error.
- [x] 2.4 Extend task deletion so deleting a task also removes its linked worktree through the provisioning port, using force mode in the first version.

## 3. Runtime wiring and verification

- [x] 3.1 Implement the web-runtime task worktree provisioner using the configured project repository, `ORA_WORK_DIR`, and the existing `ora-gitlancer` linked-worktree API.
- [x] 3.2 Wire the new provisioner into bootstrap and task service construction so HTTP task creation uses backend-owned worktree provisioning.
- [x] 3.3 Add or update unit and integration tests to cover successful task creation, provisioning failure before persistence, compensation after persistence failure, forced linked-worktree removal during task deletion, and runtime startup failure when `ORA_WORK_DIR` is missing.
