# 表字段与 Handler 功能调研

## 范围

本次调研基于数据库迁移、application 层 handler、以及 web server 路由入口，整理当前仓库里已有表字段与对应的 handler 功能。

## 总览

当前能看到的持久化表主要有 9 张：`projects`、`tasks`、`worktrees`、`virtual_folders`、`virtual_entries`、`sessions`、`artifacts`、`migrations`、`project_work_contexts`。

其中真正对外暴露完整 CRUD 或业务入口的 handler，集中在 `project`、`task`、`session`、`project_work_context` 和 `terminal` 这几组。`virtual_folders`、`virtual_entries`、`artifacts`、`migrations` 目前没有看到独立的 application handler 或 web handler，更多是领域/仓储层准备好的结构。

## 表字段与对应 handler

### 1. `projects`

字段：
- `id`
- `name`
- `root_path`
- `created_at`
- `updated_at`
- `is_deleted`

对应 handler：
- `CreateProjectHandler` -> `create_project`
- `GetProjectHandler` -> `get_project`
- `ListProjectsHandler` -> `list_projects`
- `UpdateProjectHandler` -> `update_project`
- `DeleteProjectHandler` -> `delete_project`

功能说明：
- 这是项目主表，提供完整 CRUD。
- 删除采用 soft delete，`is_deleted` 保留在记录里。
- Web 层对应 `/api/projects` 和 `/api/projects/{project_id}`。

### 2. `tasks`

字段：
- `id`
- `project_id`
- `title`
- `status`
- `worktree_id`
- `created_at`
- `updated_at`
- `is_deleted`

对应 handler：
- `CreateTaskHandler` -> `create_task`
- `GetTaskHandler` -> `get_task`
- `ListTasksHandler` -> `list_tasks`
- `UpdateTaskHandler` -> `update_task`
- `DeleteTaskHandler` -> `delete_task`

功能说明：
- 任务表同样提供完整 CRUD。
- `create_task` 和 `delete_task` 会联动 `worktrees`，负责 worktree 的创建、清理和补偿。
- Web 层对应 `/api/tasks` 和 `/api/tasks/{task_id}`。

### 3. `worktrees`

字段：
- `id`
- `task_id`
- `branch_name`
- `is_active`
- `created_at`
- `updated_at`
- `is_deleted`

对应 handler：
- 没有独立对外 CRUD handler。
- 被以下 handler 间接使用：
  - `CreateTaskHandler` / `DeleteTaskHandler` 通过 `WorktreeRepository` 创建、软删 worktree 行，并联动 Git linked-worktree checkout 的创建/删除与失败补偿。
  - `CreateTerminalSessionHandler` 在启动终端会话时通过内部私有函数 `validate_active_worktree` 校验该 worktree 仍归属于对应 task。

功能说明：
- 这是任务的附属表，主要用于管理工作区 checkout 状态。
- 它的生命周期由 task/terminal 相关 handler 驱动，而不是独立暴露给外部 API。
- 注意 `ValidateActiveWorktree` 不是 handler，而是 `crates/application/src/terminal/handlers.rs` 内的私有辅助函数，仅被 `CreateTerminalSessionHandler::handle` 调用。

### 4. `sessions`

字段：
- `id`
- `task_id`
- `agent_id`
- `agent_session_id`
- `status`
- `created_at`
- `updated_at`
- `is_deleted`

对应 handler：
- `CreateSessionHandler` -> `create_session`
- `GetSessionHandler` -> `get_session`
- `ListSessionsHandler` -> `list_sessions`
- `UpdateSessionHandler` -> `update_session`
- `DeleteSessionHandler` -> `delete_session`
- `CreateTerminalSessionHandler`（见 `crates/application/src/terminal/handlers.rs`，与 `CreateSessionHandler` 是两个平行实现，共享 `POST /api/sessions` 路由，按请求体是否含 `terminal` 字段路由分发）
- `AttachTerminalSessionHandler` -> `attach_terminal_session`
- `SendTerminalInputHandler` -> `send_terminal_input`
- `ResizeTerminalSessionHandler` -> `resize_terminal_session`
- `KillTerminalSessionHandler` -> `kill_terminal_session`
- `HandleTerminalExitHandler` -> `handle_terminal_exit`

功能说明：
- sessions 是会话主表，既支持普通会话 CRUD，也承载终端会话生命周期。
- 普通会话删除同样采用 soft delete（`DeleteSessionHandler` 调用 `soft_delete_session`，保留 `is_deleted`）。
- 终端相关 handler 会校验 session 是否为 terminal、是否仍处于运行态，并把 PTY 状态同步回 session 记录。
- Web 层 HTTP 路由：
  - `/api/sessions`：`POST` 创建普通会话或终端会话（同一端点，按请求体字段分发）；`GET` 列表。
  - `/api/sessions/{session_id}`：`GET` / `PUT` / `DELETE` 普通 CRUD。
  - `/api/sessions/{session_id}/terminal`：仅 `GET` 升级 WebSocket。
- 终端控制类 handler（`SendTerminalInputHandler` / `ResizeTerminalSessionHandler` / `KillTerminalSessionHandler` / `HandleTerminalExitHandler`）没有独立 HTTP 路由，而是通过上面那条 WebSocket 连接收发消息驱动；`HandleTerminalExitHandler` 则由 PTY runtime 生命周期事件回调驱动。

### 5. `project_work_contexts`

字段：
- `id`
- `surface`
- `window_id`
- `project_id`
- `lease_expires_at`
- `created_at`
- `updated_at`

对应 handler：
- `OpenProjectWorkContextHandler` -> `open_project_work_context`
- `RenewProjectWorkContextHandler` -> `renew_project_work_context`

功能说明：
- 这是一个带租约语义的上下文表，用于把某个客户端窗口绑定到某个项目。
- `open` 负责创建或更新上下文并处理冲突。
- `renew` 负责续租，延长 `lease_expires_at`。
- Web 层对应 open/renew 两个独立接口。
- 表上还建有 3 个索引（来自 `schema_v0002.rs`）支撑租约语义：
  - 唯一索引 `idx_project_work_contexts_surface_window (surface, window_id)`：保证每个客户端窗口同一 surface 下只占一行。
  - 查询索引 `idx_project_work_contexts_project_lease (project_id, lease_expires_at, surface, window_id)`：支撑"某项目当前是否有未过期活跃上下文"的冲突检测。
  - 索引 `idx_project_work_contexts_expiry (lease_expires_at)`：支撑过期上下文的清扫扫描。

### 6. `virtual_folders`

字段：
- `id`
- `project_id`
- `name`
- `mount_point`
- `created_at`
- `updated_at`
- `is_deleted`

对应 handler：
- 当前没有看到独立 application 或 web handler。

功能说明：
- 这是领域侧的虚拟文件夹模型，数据库里已经有表结构，但没有在当前对外 API 中形成独立入口。
- 仓库内目前没有 `VirtualFolderRepository` trait 或对应仓储实现，仅停留于 schema 层。

### 7. `virtual_entries`

字段：
- `id`
- `virtual_folder_id`
- `parent_entry_id`
- `name`
- `kind`
- `content_ref`
- `created_at`
- `updated_at`
- `is_deleted`

对应 handler：
- 当前没有看到独立 application 或 web handler。

功能说明：
- 这是虚拟文件树节点表，字段里已经包含层级关系和内容引用。
- `content_ref` 在逻辑上指向 `artifacts`，但 schema 未声明外键约束（仅 `TEXT` 列），关联靠应用层维护。
- 仓库内目前没有 `VirtualEntryRepository` trait 或对应仓储实现，仅停留于 schema 层。

### 8. `artifacts`

字段：
- `id`
- `task_id`
- `content`
- `created_at`
- `updated_at`
- `is_deleted`

对应 handler：
- 当前没有看到独立 application 或 web handler。

功能说明：
- 这是任务输出或中间内容的持久化表。
- 仓库内目前没有 `ArtifactRepository` trait 或对应仓储实现，仅停留于 schema 层，是目前内容树的底座，而非已暴露给 API 的功能面。

### 9. `migrations`

字段：
- `version`
- `executed_at`

对应 handler：
- 没有业务 handler。

功能说明：
- 这是数据库迁移 bookkeeping 表，只负责记录已执行版本。

## 结论

从当前代码看，真正已经形成“表 -> handler -> web route”闭环的，是 `projects`、`tasks`、`sessions` 和 `project_work_contexts`。`worktrees` 由 task/terminal handler 间接管理，`virtual_folders`、`virtual_entries`、`artifacts`、`migrations` 目前仍停留在模型/表结构层。

## 主要依据文件

- `crates/db/src/migration/schema_v0001.rs`
- `crates/db/src/migration/schema_v0002.rs`
- `crates/application/src/project/handlers.rs`
- `crates/application/src/task/handlers.rs`
- `crates/application/src/session/handlers.rs`
- `crates/application/src/project_work_context/handlers.rs`
- `crates/application/src/terminal/handlers.rs`
- `apps/web/server/src/routes.rs`
- `apps/web/server/src/handlers/projects.rs`
- `apps/web/server/src/handlers/tasks.rs`
- `apps/web/server/src/handlers/sessions.rs`
- `apps/web/server/src/handlers/project_work_contexts.rs`
