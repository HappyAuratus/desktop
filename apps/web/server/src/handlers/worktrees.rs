use crate::app_state::AppState;
use crate::error::WebApiError;
use axum::Json;
use axum::extract::{Path, State};
use ora_contracts::{
    CreateWorktreeRequest, CreateWorktreeResponse, DeleteWorktreeRequest, DeleteWorktreeResponse,
    GetWorktreeRequest, GetWorktreeResponse, ListWorktreesRequest, ListWorktreesResponse,
    UpdateWorktreeRequest, UpdateWorktreeResponse, WorktreeActivity,
};
use serde::Deserialize;

/// Carries the request path segment used by worktree identifier routes.
#[derive(Debug, Deserialize)]
pub struct WorktreePath {
    worktree_id: String,
}

/// Carries the HTTP body used for worktree update routes before the path identifier is applied.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorktreeBody {
    task_id: String,
    branch_name: Option<String>,
    activity: WorktreeActivity,
}

/// Creates one worktree by forwarding the request body into the application layer.
pub async fn create_worktree(
    State(app_state): State<AppState>,
    Json(request): Json<CreateWorktreeRequest>,
) -> Result<Json<CreateWorktreeResponse>, WebApiError> {
    app_state
        .worktree_api()
        .create_worktree(request)
        .map(Json)
        .map_err(WebApiError::from)
}

/// Loads one worktree by combining the path identifier into the contract request.
pub async fn get_worktree(
    State(app_state): State<AppState>,
    Path(path): Path<WorktreePath>,
) -> Result<Json<GetWorktreeResponse>, WebApiError> {
    app_state
        .worktree_api()
        .get_worktree(GetWorktreeRequest {
            worktree_id: path.worktree_id,
        })
        .map(Json)
        .map_err(WebApiError::from)
}

/// Lists every visible worktree by delegating to the application handler.
pub async fn list_worktrees(
    State(app_state): State<AppState>,
) -> Result<Json<ListWorktreesResponse>, WebApiError> {
    app_state
        .worktree_api()
        .list_worktrees(ListWorktreesRequest {})
        .map(Json)
        .map_err(WebApiError::from)
}

/// Replaces one worktree by combining the route identifier with the JSON body payload.
pub async fn update_worktree(
    State(app_state): State<AppState>,
    Path(path): Path<WorktreePath>,
    Json(body): Json<UpdateWorktreeBody>,
) -> Result<Json<UpdateWorktreeResponse>, WebApiError> {
    app_state
        .worktree_api()
        .update_worktree(UpdateWorktreeRequest {
            worktree_id: path.worktree_id,
            task_id: body.task_id,
            branch_name: body.branch_name,
            activity: body.activity,
        })
        .map(Json)
        .map_err(WebApiError::from)
}

/// Deletes one worktree by combining the path identifier into the contract request.
pub async fn delete_worktree(
    State(app_state): State<AppState>,
    Path(path): Path<WorktreePath>,
) -> Result<Json<DeleteWorktreeResponse>, WebApiError> {
    app_state
        .worktree_api()
        .delete_worktree(DeleteWorktreeRequest {
            worktree_id: path.worktree_id,
        })
        .map(Json)
        .map_err(WebApiError::from)
}
