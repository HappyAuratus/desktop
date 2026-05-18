## ADDED Requirements

### Requirement: Task create contracts SHALL reserve worktree selection to the backend
The system SHALL define `CreateTaskRequest` so callers provide task identity inputs only: `project_id`, `title`, and `status`. The create-task contract SHALL NOT accept a caller-supplied `worktree_id`, because task worktree assignment is an internal backend responsibility. The returned shared `Task` view SHALL continue to expose the assigned `worktree_id`.

#### Scenario: Adapter submits a task creation request
- **WHEN** an HTTP or Tauri adapter constructs a `CreateTaskRequest`
- **THEN** the request shape excludes `worktree_id` and includes only the project, title, and status fields required to create the task

#### Scenario: Caller receives a created task
- **WHEN** a create-task use case returns successfully
- **THEN** the shared `Task` response payload includes the backend-assigned `worktree_id`
