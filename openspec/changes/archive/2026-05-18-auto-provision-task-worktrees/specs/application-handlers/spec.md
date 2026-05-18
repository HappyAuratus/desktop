## ADDED Requirements

### Requirement: CreateTaskHandler SHALL orchestrate task-owned worktree provisioning
The system SHALL require `CreateTaskHandler` to generate the task identifier, derive the task-owned branch name and worktree root, delegate linked-worktree provisioning through an application-owned port, persist the worktree record, and persist the task record with the resulting `worktree_id`. This orchestration SHALL remain transport-agnostic and SHALL NOT move Git-specific types into request or response contracts.

#### Scenario: Handler creates a task successfully
- **WHEN** `CreateTaskHandler` receives a valid task creation request
- **THEN** it orchestrates linked-worktree provisioning and persistence before returning the shared created-task response

#### Scenario: Handler unit test replaces Git provisioning
- **WHEN** a unit test constructs `CreateTaskHandler` with fake repositories and a fake provisioning dependency
- **THEN** the complete task-create flow can be exercised without a real Git repository or filesystem side effects

### Requirement: CreateTaskHandler SHALL treat provisioning failures as stable application failures
The system SHALL require `CreateTaskHandler` to convert provisioning and compensating-cleanup failures into stable application errors and structured logs, and it SHALL avoid persisting partial task state when those failures occur.

#### Scenario: Provisioning dependency fails before persistence
- **WHEN** the task worktree provisioning port returns a failure before any task row is created
- **THEN** `CreateTaskHandler` returns an application failure and logs the create-task failure context

#### Scenario: Compensation is needed after a persistence failure
- **WHEN** worktree or task persistence fails after linked-worktree creation succeeded
- **THEN** `CreateTaskHandler` attempts cleanup through the same provisioning port and returns a stable application failure outcome
