//! Orchestrates human completion of an interactive workflow node.
//!
//! The core (validating, assembling the conversation, diffing the worktree) lives here so the
//! future agent/CLI path can reuse the same commit through the engine. The session stop and the
//! engine commit are done by the caller around [`prepare_completion`].

use crate::agent_runtime::AgentRuntimeManager;
use crate::error::BackendError;
use crate::workflow_run_executor::{capture_worktree_snapshot, compute_file_changes};
use agent_client_protocol_schema::v1::{ContentBlock, SessionUpdate};
use ora_application::{ApplicationError, FileChange, WorkflowGraph, WorkflowRunEngineRepository};
use ora_db::{RepositoryPool, SqliteWorkflowRunEngineRepository};
use ora_domain::{
    SessionId, WorkflowNodeRunId, WorkflowNodeStatus, WorkflowRunId, WorkflowRunStatus,
};
use ora_history::{HistoryRecord, SessionHistory, read_session_history};
use ora_logging::ora_warn;
use std::collections::BTreeMap;
use std::path::Path;

/// The assembled result of validating and preparing one interactive node completion.
pub(crate) struct PreparedCompletion {
    pub node_run_id: WorkflowNodeRunId,
    pub output: Option<String>,
    pub file_changes: Vec<FileChange>,
    pub session_id: Option<SessionId>,
}

/// Validates an interactive node and assembles its completion output from persisted state.
///
/// The run must be `Running`, the node must be awaiting input (`Pending`) and interactive, and
/// the final assistant output is read from the session's durable history. Intended to run in a
/// blocking closure so the async runtime is not held.
pub(crate) fn prepare_completion(
    pool: &RepositoryPool,
    sessions_root: &Path,
    baselines_root: &Path,
    agent_runtime: &AgentRuntimeManager,
    run_id: &WorkflowRunId,
    node_id: &str,
) -> Result<PreparedCompletion, BackendError> {
    let repository = SqliteWorkflowRunEngineRepository::new(pool.clone());
    let context = repository
        .find_execution_context(run_id)
        .map_err(repository_error)?
        .ok_or_else(|| ApplicationError::WorkflowRunNotFound {
            run_id: run_id.to_string(),
        })?;
    if context.run.status != WorkflowRunStatus::Running {
        return Err(ApplicationError::WorkflowNodeNotAwaitingInput {
            node_id: node_id.to_string(),
        }
        .into());
    }
    let node_run = repository
        .list_node_runs(run_id)
        .map_err(repository_error)?
        .into_iter()
        .find(|node_run| node_run.node_id == node_id)
        .ok_or_else(|| ApplicationError::WorkflowNodeNotFound {
            node_id: node_id.to_string(),
        })?;
    if node_run.status != WorkflowNodeStatus::Pending {
        return Err(ApplicationError::WorkflowNodeNotAwaitingInput {
            node_id: node_id.to_string(),
        }
        .into());
    }
    // Defensive: a `Pending` node run can only be produced by an interactive node, but the
    // completion contract also requires the frozen graph to declare it interactive.
    let graph = WorkflowGraph::parse(&context.graph_json)
        .map_err(ApplicationError::WorkflowRunGraphParse)?;
    let interactive = graph
        .node(&node_run.node_id)
        .and_then(|node| node.agent_config.as_ref())
        .is_some_and(|config| config.interactive);
    if !interactive {
        return Err(ApplicationError::WorkflowNodeNotAwaitingInput {
            node_id: node_id.to_string(),
        }
        .into());
    }

    let output = node_run
        .session_id
        .as_ref()
        .map(|session_id| -> Result<Option<String>, BackendError> {
            let history = read_session_history(sessions_root, session_id.as_ref())
                .map_err(|error| BackendError::internal("failed to read session history", error))?;
            Ok(assistant_output_from_history(&history))
        })
        .transpose()?
        .flatten();

    let file_changes = match load_worktree_baseline(baselines_root, &node_run.id) {
        Some(baseline) => {
            let worktree_root = agent_runtime.task_cwd(&context.task.id)?;
            compute_file_changes(&baseline, &capture_worktree_snapshot(&worktree_root))
        }
        None => {
            // A missing baseline means the node's changes cannot be attributed; reporting an
            // empty diff is safer than claiming the whole tree changed (D9/E5).
            ora_warn!(
                node_run_id = %node_run.id,
                "completing interactive node without a persisted worktree baseline; reporting no file changes"
            );
            Vec::new()
        }
    };

    Ok(PreparedCompletion {
        node_run_id: node_run.id,
        output,
        file_changes,
        session_id: node_run.session_id,
    })
}

/// Returns the final settled assistant message from a session's durable history.
fn assistant_output_from_history(history: &SessionHistory) -> Option<String> {
    history.lines.iter().rev().find_map(|line| {
        let HistoryRecord::Update { update } = &line.record else {
            return None;
        };
        assistant_text(update.as_ref()).map(str::to_string)
    })
}

/// Extracts text from one settled assistant message update.
fn assistant_text(update: &SessionUpdate) -> Option<&str> {
    let content = match update {
        SessionUpdate::AgentMessageChunk(chunk) => &chunk.content,
        _ => return None,
    };
    let ContentBlock::Text(text) = content else {
        return None;
    };
    Some(&text.text)
}

/// Loads the worktree baseline persisted when an interactive node started, or `None` when the
/// file is missing or unreadable (the completion then reports no file changes).
fn load_worktree_baseline(
    baselines_root: &Path,
    node_run_id: &WorkflowNodeRunId,
) -> Option<BTreeMap<String, Option<String>>> {
    let path = baselines_root.join(format!("{}.json", node_run_id.as_ref()));
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Maps a workflow engine repository failure onto the public backend error.
fn repository_error(source: ora_application::RepositoryError) -> BackendError {
    BackendError::from(ApplicationError::WorkflowRunRepository { source })
}

#[cfg(test)]
mod tests {
    use super::assistant_output_from_history;
    use agent_client_protocol_schema::v1::{
        ContentBlock, ContentChunk, SessionUpdate, TextContent,
    };
    use ora_domain::AgentCli;
    use ora_history::{HistoryIntegrity, HistoryLine, HistoryRecord, SessionHistory, SessionMeta};
    use pretty_assertions::assert_eq;

    fn text_update(role: &str, text: &str) -> HistoryLine {
        let update = match role {
            "user" => SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(text.to_string()),
            ))),
            _ => SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(text.to_string()),
            ))),
        };
        HistoryLine::new(
            "2026-08-18T00:00:00+08:00",
            0,
            HistoryRecord::Update {
                update: Box::new(update),
            },
        )
    }

    fn history(lines: Vec<HistoryLine>) -> SessionHistory {
        let next_seq = lines.len() as u32;
        SessionHistory {
            lines,
            next_seq,
            integrity: HistoryIntegrity::Complete,
        }
    }

    /// A multi-turn interactive node persists only its final assistant message as node output.
    #[test]
    fn assistant_output_from_history_returns_the_final_assistant_message() {
        let history = history(vec![
            text_update("user", "review the plan"),
            text_update("assistant", "here is v1"),
            text_update("user", "keep section two"),
            text_update("assistant", "v1 is final"),
        ]);
        assert_eq!(
            assistant_output_from_history(&history),
            Some("v1 is final".to_string())
        );
    }

    /// Non-message and user-only history yields no assistant node output.
    #[test]
    fn assistant_output_from_history_skips_non_assistant_records() {
        let meta = HistoryLine::new(
            "2026-08-18T00:00:00+08:00",
            0,
            HistoryRecord::Meta(SessionMeta {
                schema_version: 1,
                session_id: "session-1".to_string(),
                task_id: "task-1".to_string(),
                agent_cli: AgentCli::OpenCode,
                agent_session_id: "provider-1".to_string(),
                cwd: std::path::PathBuf::from("."),
            }),
        );
        assert_eq!(
            assistant_output_from_history(&history(vec![
                meta,
                text_update("user", "still working"),
            ])),
            None
        );
    }
}
