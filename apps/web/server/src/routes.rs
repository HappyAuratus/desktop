use crate::app_state::AppState;
use crate::handlers::{health, projects, sessions, tasks, worktrees};
use axum::Router;
use axum::routing::{get, post};

/// Builds the top-level router for health checks and the persisted CRUD routes.
pub fn build_router(app_state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .route(
            "/api/projects",
            post(projects::create_project).get(projects::list_projects),
        )
        .route(
            "/api/projects/{project_id}",
            get(projects::get_project)
                .put(projects::update_project)
                .delete(projects::delete_project),
        )
        .route(
            "/api/tasks",
            post(tasks::create_task).get(tasks::list_tasks),
        )
        .route(
            "/api/tasks/{task_id}",
            get(tasks::get_task)
                .put(tasks::update_task)
                .delete(tasks::delete_task),
        )
        .route(
            "/api/worktrees",
            post(worktrees::create_worktree).get(worktrees::list_worktrees),
        )
        .route(
            "/api/worktrees/{worktree_id}",
            get(worktrees::get_worktree)
                .put(worktrees::update_worktree)
                .delete(worktrees::delete_worktree),
        )
        .route(
            "/api/sessions",
            post(sessions::create_session).get(sessions::list_sessions),
        )
        .route(
            "/api/sessions/{session_id}",
            get(sessions::get_session)
                .put(sessions::update_session)
                .delete(sessions::delete_session),
        )
        .with_state(app_state)
}

#[cfg(test)]
mod tests {
    use super::build_router;
    use crate::bootstrap::build_app_state_for_database;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode};
    use pretty_assertions::assert_eq;
    use serde_json::{Value, json};
    use tempfile::TempDir;
    use tower::util::ServiceExt;

    /// Verifies the liveness route reports process health without bootstrap state.
    #[tokio::test]
    async fn serves_liveness_route() {
        let (_temp_dir, app) = test_router();
        let response = match app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/health/live")
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Verifies readiness stays unavailable until bootstrap marks the state as ready.
    #[tokio::test]
    async fn serves_unready_status_before_bootstrap_completion() {
        let temp_dir = TempDir::new().unwrap();
        let database_path = temp_dir.path().join("ready.sqlite3");
        let app_state = build_app_state_for_database(&database_path).unwrap_or_else(|error| {
            panic!("expected application state bootstrap to succeed: {error}");
        });
        let app = build_router(app_state);
        let response = match app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/health/ready")
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Verifies the router supports the persisted project CRUD slice end to end.
    #[tokio::test]
    async fn serves_project_crud_routes() {
        let (_temp_dir, app) = test_router();
        let create_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/projects")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Ora",
                            "rootPath": "/workspace/ora",
                        })
                        .to_string(),
                    ))
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let created_project = response_json(create_response).await["project"].clone();
        let project_id = match created_project["id"].as_str() {
            Some(project_id) => project_id.to_string(),
            None => panic!("response did not include a project id"),
        };
        let list_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/projects")
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let get_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/projects/{project_id}"))
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let update_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/api/projects/{project_id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Ora Updated",
                            "rootPath": "/workspace/ora-next",
                        })
                        .to_string(),
                    ))
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let delete_response = match app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/api/projects/{project_id}"))
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };

        assert_eq!(
            created_project,
            json!({
                "id": project_id,
                "name": "Ora",
                "rootPath": "/workspace/ora",
            })
        );
        assert_eq!(
            response_json(list_response).await,
            json!({
                "projects": [
                    {
                        "id": project_id,
                        "name": "Ora",
                        "rootPath": "/workspace/ora",
                    },
                ],
            })
        );
        assert_eq!(
            response_json(get_response).await,
            json!({
                "project": {
                    "id": project_id,
                    "name": "Ora",
                    "rootPath": "/workspace/ora",
                },
            })
        );
        assert_eq!(
            response_json(update_response).await,
            json!({
                "project": {
                    "id": project_id,
                    "name": "Ora Updated",
                    "rootPath": "/workspace/ora-next",
                },
            })
        );
        assert_eq!(
            response_json(delete_response).await,
            json!({
                "projectId": project_id,
            })
        );
    }

    /// Verifies missing projects surface the shared HTTP error payload.
    #[tokio::test]
    async fn serves_not_found_payload_for_missing_project() {
        let (_temp_dir, app) = test_router();
        let response = match app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/projects/missing-project")
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response_json(response).await,
            json!({
                "error": {
                    "code": "project_not_found",
                    "message": "project not found: missing-project",
                },
            })
        );
    }

    /// Verifies the router supports task CRUD routes end to end.
    #[tokio::test]
    async fn serves_task_crud_routes() {
        let (_temp_dir, app) = test_router();
        let create_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/tasks")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "projectId": "project-1",
                            "title": "Ship handlers",
                            "status": "todo",
                            "worktreeId": null,
                        })
                        .to_string(),
                    ))
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let created_task = response_json(create_response).await["task"].clone();
        let task_id = match created_task["id"].as_str() {
            Some(task_id) => task_id.to_string(),
            None => panic!("response did not include a task id"),
        };
        let list_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/tasks")
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let get_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/tasks/{task_id}"))
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let update_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/api/tasks/{task_id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "projectId": "project-2",
                            "title": "Ship updated handlers",
                            "status": "doing",
                            "worktreeId": "worktree-1",
                        })
                        .to_string(),
                    ))
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let delete_response = match app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/api/tasks/{task_id}"))
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };

        assert_eq!(
            created_task,
            json!({
                "id": task_id,
                "projectId": "project-1",
                "title": "Ship handlers",
                "status": "todo",
                "worktreeId": null,
            })
        );
        assert_eq!(
            response_json(list_response).await,
            json!({
                "tasks": [
                    {
                        "id": task_id,
                        "projectId": "project-1",
                        "title": "Ship handlers",
                        "status": "todo",
                        "worktreeId": null,
                    },
                ],
            })
        );
        assert_eq!(
            response_json(get_response).await,
            json!({
                "task": {
                    "id": task_id,
                    "projectId": "project-1",
                    "title": "Ship handlers",
                    "status": "todo",
                    "worktreeId": null,
                },
            })
        );
        assert_eq!(
            response_json(update_response).await,
            json!({
                "task": {
                    "id": task_id,
                    "projectId": "project-2",
                    "title": "Ship updated handlers",
                    "status": "doing",
                    "worktreeId": "worktree-1",
                },
            })
        );
        assert_eq!(
            response_json(delete_response).await,
            json!({
                "taskId": task_id,
            })
        );
    }

    /// Verifies the router supports worktree CRUD routes end to end.
    #[tokio::test]
    async fn serves_worktree_crud_routes() {
        let (_temp_dir, app) = test_router();
        let create_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/worktrees")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "taskId": "task-1",
                            "branchName": "feature/task-handlers",
                            "activity": "active",
                        })
                        .to_string(),
                    ))
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let created_worktree = response_json(create_response).await["worktree"].clone();
        let worktree_id = match created_worktree["id"].as_str() {
            Some(worktree_id) => worktree_id.to_string(),
            None => panic!("response did not include a worktree id"),
        };
        let list_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/worktrees")
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let get_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/worktrees/{worktree_id}"))
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let update_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/api/worktrees/{worktree_id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "taskId": "task-2",
                            "branchName": null,
                            "activity": "inactive",
                        })
                        .to_string(),
                    ))
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let delete_response = match app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/api/worktrees/{worktree_id}"))
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };

        assert_eq!(
            created_worktree,
            json!({
                "id": worktree_id,
                "taskId": "task-1",
                "branchName": "feature/task-handlers",
                "activity": "active",
            })
        );
        assert_eq!(
            response_json(list_response).await,
            json!({
                "worktrees": [
                    {
                        "id": worktree_id,
                        "taskId": "task-1",
                        "branchName": "feature/task-handlers",
                        "activity": "active",
                    },
                ],
            })
        );
        assert_eq!(
            response_json(get_response).await,
            json!({
                "worktree": {
                    "id": worktree_id,
                    "taskId": "task-1",
                    "branchName": "feature/task-handlers",
                    "activity": "active",
                },
            })
        );
        assert_eq!(
            response_json(update_response).await,
            json!({
                "worktree": {
                    "id": worktree_id,
                    "taskId": "task-2",
                    "branchName": null,
                    "activity": "inactive",
                },
            })
        );
        assert_eq!(
            response_json(delete_response).await,
            json!({
                "worktreeId": worktree_id,
            })
        );
    }

    /// Verifies the router supports session CRUD routes end to end.
    #[tokio::test]
    async fn serves_session_crud_routes() {
        let (_temp_dir, app) = test_router();
        let create_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/sessions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "taskId": "task-1",
                            "agentId": "agent-1",
                            "agentSessionId": "provider-1",
                            "status": "running",
                        })
                        .to_string(),
                    ))
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let created_session = response_json(create_response).await["session"].clone();
        let session_id = match created_session["id"].as_str() {
            Some(session_id) => session_id.to_string(),
            None => panic!("response did not include a session id"),
        };
        let list_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let get_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let update_response = match app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/api/sessions/{session_id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "taskId": "task-2",
                            "agentId": "agent-2",
                            "agentSessionId": null,
                            "status": "stopped",
                        })
                        .to_string(),
                    ))
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };
        let delete_response = match app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap_or_else(|error| panic!("failed to build request: {error}")),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request failed: {error}"),
        };

        assert_eq!(
            created_session,
            json!({
                "id": session_id,
                "taskId": "task-1",
                "agentId": "agent-1",
                "agentSessionId": "provider-1",
                "status": "running",
            })
        );
        assert_eq!(
            response_json(list_response).await,
            json!({
                "sessions": [
                    {
                        "id": session_id,
                        "taskId": "task-1",
                        "agentId": "agent-1",
                        "agentSessionId": "provider-1",
                        "status": "running",
                    },
                ],
            })
        );
        assert_eq!(
            response_json(get_response).await,
            json!({
                "session": {
                    "id": session_id,
                    "taskId": "task-1",
                    "agentId": "agent-1",
                    "agentSessionId": "provider-1",
                    "status": "running",
                },
            })
        );
        assert_eq!(
            response_json(update_response).await,
            json!({
                "session": {
                    "id": session_id,
                    "taskId": "task-2",
                    "agentId": "agent-2",
                    "agentSessionId": null,
                    "status": "stopped",
                },
            })
        );
        assert_eq!(
            response_json(delete_response).await,
            json!({
                "sessionId": session_id,
            })
        );
    }

    /// Builds a ready router for tests that need the full persisted route surface.
    fn test_router() -> (TempDir, axum::Router) {
        let temp_dir = TempDir::new().unwrap();
        let database_path = temp_dir.path().join("routes.sqlite3");
        let app_state = build_app_state_for_database(&database_path).unwrap_or_else(|error| {
            panic!("expected application state bootstrap to succeed: {error}");
        });
        app_state.mark_ready();

        (temp_dir, build_router(app_state))
    }

    /// Decodes one JSON response body so route tests can compare the full payload.
    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = match to_bytes(response.into_body(), usize::MAX).await {
            Ok(bytes) => bytes,
            Err(error) => panic!("failed to read response body: {error}"),
        };

        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => value,
            Err(error) => panic!("failed to decode JSON body: {error}"),
        }
    }
}
