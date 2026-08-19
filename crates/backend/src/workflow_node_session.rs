//! Coordinates human follow-up turns against interactive workflow node sessions.
//!
//! An interactive node parks at `Pending` (awaiting input) after its first turn. When the user
//! sends a follow-up message through the ordinary `prompt_session` path, this module flips the
//! owning node to `Running` while the agent answers and back to `Pending` when the turn ends or
//! the stream is dropped. The repository guard makes a stale flip against a completed or
//! cancelled node a no-op.

use crate::clock::SystemClock;
use crate::error::BackendError;
use ora_application::{ApplicationError, Clock, RepositoryError, WorkflowRunEngineRepository};
use ora_db::{RepositoryPool, SqliteWorkflowRunEngineRepository};
use ora_domain::{SessionId, WorkflowNodeRun, WorkflowNodeRunId, WorkflowNodeStatus};

/// Whether a session-bound node run should flip to `Running` for a prompt turn.
///
/// Only a `Pending` interactive node (the sole producer of `Pending` node runs) participates; a
/// session bound to no node, or to a terminal node, proceeds as an ordinary session prompt.
pub(crate) fn awaiting_node_for_turn(
    node_run: Option<&WorkflowNodeRun>,
) -> Option<&WorkflowNodeRun> {
    node_run.filter(|node_run| node_run.status == WorkflowNodeStatus::Pending)
}

/// Flips an awaiting interactive node to `Running` before a human turn, returning the node-run id
/// that must be flipped back when the turn ends. `None` when the session is not bound to an
/// awaiting node, in which case the prompt proceeds as an ordinary session prompt.
pub(crate) async fn begin_human_turn(
    pool: &RepositoryPool,
    session_id: &str,
) -> Result<Option<WorkflowNodeRunId>, BackendError> {
    let pool = pool.clone();
    let session_id = SessionId::new(session_id);
    tokio::task::spawn_blocking(move || {
        let repository = SqliteWorkflowRunEngineRepository::new(pool);
        let node_run = repository
            .find_node_run_by_session_id(&session_id)
            .map_err(repository_error)?;
        let Some(node_run) = awaiting_node_for_turn(node_run.as_ref()) else {
            return Ok(None);
        };
        repository
            .transition_node_run_status(
                &node_run.id,
                WorkflowNodeStatus::Pending,
                WorkflowNodeStatus::Running,
                SystemClock.now_timestamp_millis(),
            )
            .map_err(repository_error)?;
        Ok(Some(node_run.id.clone()))
    })
    .await
    .map_err(|source| BackendError::internal("repository operation did not complete", source))?
}

/// Flips an interactive node back to `Pending` when a turn ends or its stream is dropped.
///
/// Called fire-and-forget from the stream's drop hook; the repository guard means a stale flip
/// against a completed or cancelled node is a clean no-op.
pub(crate) async fn end_human_turn(
    pool: &RepositoryPool,
    node_run_id: &WorkflowNodeRunId,
) -> Result<(), BackendError> {
    let pool = pool.clone();
    let node_run_id = node_run_id.clone();
    tokio::task::spawn_blocking(move || {
        let repository = SqliteWorkflowRunEngineRepository::new(pool);
        repository
            .transition_node_run_status(
                &node_run_id,
                WorkflowNodeStatus::Running,
                WorkflowNodeStatus::Pending,
                SystemClock.now_timestamp_millis(),
            )
            .map_err(repository_error)?;
        Ok(())
    })
    .await
    .map_err(|source| BackendError::internal("repository operation did not complete", source))?
}

/// Maps a workflow engine repository failure onto the public backend error.
fn repository_error(source: RepositoryError) -> BackendError {
    BackendError::from(ApplicationError::WorkflowRunRepository { source })
}

#[cfg(test)]
mod tests {
    use super::awaiting_node_for_turn;
    use ora_domain::{
        AuditFields, SessionId, WorkflowNodeRun, WorkflowNodeRunId, WorkflowNodeStatus,
        WorkflowRunId,
    };
    use pretty_assertions::assert_eq;

    fn node_run(status: WorkflowNodeStatus) -> WorkflowNodeRun {
        WorkflowNodeRun::new(
            WorkflowNodeRunId::new("node-1"),
            WorkflowRunId::new("run-1"),
            "a",
            "agent",
            Some(SessionId::new("session-1")),
            status,
            None,
            None,
            None,
            None,
            Some(30),
            None,
            AuditFields::new(30, 30, false),
        )
    }

    /// Only a `Pending` node run (an awaiting interactive node) participates in the turn flip.
    #[test]
    fn awaiting_node_for_turn_selects_only_pending_nodes() {
        assert_eq!(awaiting_node_for_turn(None).map(|node| node.status), None);
        assert_eq!(
            awaiting_node_for_turn(Some(&node_run(WorkflowNodeStatus::Pending)))
                .map(|node| node.status),
            Some(WorkflowNodeStatus::Pending)
        );
        for status in [
            WorkflowNodeStatus::Running,
            WorkflowNodeStatus::Succeeded,
            WorkflowNodeStatus::Failed,
            WorkflowNodeStatus::Cancelled,
        ] {
            assert_eq!(
                awaiting_node_for_turn(Some(&node_run(status))).map(|node| node.status),
                None,
                "{status:?} must not flip for a prompt turn"
            );
        }
    }
}
