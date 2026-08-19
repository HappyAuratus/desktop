use crate::agent_runtime::{AgentRuntimeManager, WarmOwner};
use crate::clock::SystemClock;
use crate::error::BackendError;
use crate::workflow_run_prerequisites::resolve_executable_skill_name;
use crate::workflow_run_prompt::{WorkflowPromptRequest, assemble_workflow_prompt};
use agent_client_protocol_schema::v1::ContentBlock;
use agent_client_protocol_schema::v1::SessionUpdate;
use agent_client_protocol_schema::v1::StopReason;
use agent_client_protocol_schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption,
    SessionConfigSelectOptions,
};
use ora_application::{
    AgentDefinitionRepository, AgentSkill, Clock, ExecutionContext, FileChange,
    FilesystemSkillStorage, NodeExecutor, RepositoryError, WorkflowGraphNode, WorkflowRunCallback,
    WorkflowRunEngineRepository,
};
use ora_contracts::{
    AgentCli as ContractAgentCli, AttachSessionRequest, PromptSessionEvent, PromptSessionRequest,
    SetSessionConfigRequest, StopSessionRequest, WarmSessionRequest, WarmSessionTarget,
    WorkflowRunLocale,
};
use ora_db::{
    RepositoryPool, SqliteAgentDefinitionRepository, SqliteSkillRepository,
    SqliteWorkflowRunEngineRepository,
};
use ora_domain::{
    AgentDefinitionId, Namespace, SessionId, WorkflowNodeRunId, WorkflowNodeStatus, WorkflowRunId,
};
use serde::Deserialize;
use similar::{ChangeTag, TextDiff};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use thiserror::Error;

/// Executes one agent node through a real Ora session, reporting completion to the run engine.
///
/// `dispatch` spawns a background task that warms, attaches, configures, prompts, and stops one
/// dedicated session per node, then reports the result through the `WorkflowRunCallback`.
#[derive(Clone)]
pub struct WorkflowRunNodeExecutor {
    agent_runtime: Arc<AgentRuntimeManager>,
    pool: RepositoryPool,
    /// Skill catalog root used to resolve an enabled skill's executable `/name` for the prompt.
    skills_root: PathBuf,
    agent_repository: SqliteAgentDefinitionRepository,
    callback: Arc<dyn WorkflowRunCallback>,
    clock: SystemClock,
    /// Root for the per-node worktree baseline snapshots an interactive node diffs at completion.
    baselines_root: PathBuf,
}

impl WorkflowRunNodeExecutor {
    /// Builds an executor from the session runtime, persistence, skill catalog, role catalog, and
    /// engine callback.
    pub fn new(
        agent_runtime: Arc<AgentRuntimeManager>,
        pool: RepositoryPool,
        skills_root: PathBuf,
        agent_repository: SqliteAgentDefinitionRepository,
        callback: Arc<dyn WorkflowRunCallback>,
        clock: SystemClock,
        baselines_root: PathBuf,
    ) -> Self {
        Self {
            agent_runtime,
            pool,
            skills_root,
            agent_repository,
            callback,
            clock,
            baselines_root,
        }
    }
}

impl NodeExecutor for WorkflowRunNodeExecutor {
    fn dispatch(
        &self,
        node_run_id: &WorkflowNodeRunId,
        node: &WorkflowGraphNode,
        context: &ExecutionContext,
    ) {
        let agent_runtime = self.agent_runtime.clone();
        let pool = self.pool.clone();
        let skills_root = self.skills_root.clone();
        let agent_repository = self.agent_repository.clone();
        let callback = self.callback.clone();
        let clock = self.clock;
        let baselines_root = self.baselines_root.clone();
        let node_run_id = node_run_id.clone();
        let node = node.clone();
        let context = context.clone();
        tokio::spawn(async move {
            match drive_agent_node(
                &agent_runtime,
                &pool,
                &skills_root,
                &agent_repository,
                &clock,
                &baselines_root,
                &node_run_id,
                &node,
                &context,
            )
            .await
            {
                Ok(outcome) => report_outcome(&callback, &context.run.id, &node_run_id, outcome),
                Err(error) => {
                    callback.fail_node(&context.run.id, &node_run_id, error.message(), None)
                }
            }
        });
    }
}

/// The result of one driven agent node turn.
enum AgentNodeOutcome {
    /// The node finished and reports completion or failure through the callback.
    Completed {
        output: Option<String>,
        stop_reason: StopReason,
        file_changes: Vec<FileChange>,
    },
    /// An interactive node's first turn ended naturally; it parks at `Pending` awaiting input.
    AwaitingInput,
}

/// Failures raised while driving one agent node's session.
#[derive(Debug, Error)]
pub enum NodeExecutionError {
    #[error("agent CLI {agent_cli} is not supported")]
    UnknownAgentCli { agent_cli: String },
    #[error("model {model_id} is not advertised by agent CLI {agent_cli}")]
    WorkflowModelNotFound { agent_cli: String, model_id: String },
    #[error("agent node {node_id} has no agent configuration")]
    MissingAgentConfig { node_id: String },
    #[error("enabled skill {skill_id} could not be resolved to an executable name")]
    SkillResolution { skill_id: String },
    #[error("prompt session ended without a stop reason")]
    SessionEndedWithoutStopReason,
    #[error("failed to persist worktree baseline for node {node_id}: {source}")]
    BaselinePersist {
        node_id: String,
        #[source]
        source: std::io::Error,
    },
    #[error("workflow run repository operation failed")]
    Repository(#[from] RepositoryError),
    #[error("session failed: {0}")]
    Session(#[from] BackendError),
}

impl NodeExecutionError {
    /// Renders the actionable message surfaced to the failed node.
    fn message(&self) -> String {
        self.to_string()
    }
}

/// Captures every worktree-visible file (tracked and untracked, gitignore-respecting) as
/// worktree-relative path → content.
///
/// Unlike `git status --porcelain`, which folds untracked directories into a single `?? dir/`
/// entry and omits clean tracked files, `git ls-files -co` expands both: a node that creates
/// files inside a new directory (e.g. `openspec/...`) or edits an already-committed file for the
/// first time still shows up in the before/after delta. Best-effort — when git is unavailable
/// the snapshot is empty, so a node that cannot diff its worktree records no file changes.
pub(crate) fn capture_worktree_snapshot(worktree_root: &Path) -> BTreeMap<String, Option<String>> {
    let Ok(output) = Command::new("git")
        .args(["ls-files", "-co", "--exclude-standard", "-z"])
        .current_dir(worktree_root)
        .output()
    else {
        return BTreeMap::new();
    };
    let mut snapshot = BTreeMap::new();
    for path in String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|entry| !entry.is_empty())
    {
        // A file deleted since it was committed reads as `None`; a directory entry can only
        // appear for an in-index submodule, which we deliberately skip as non-line-content.
        let content = std::fs::read_to_string(worktree_root.join(path)).ok();
        snapshot.insert(path.to_string(), content);
    }
    snapshot
}

/// Diffs the worktree state captured before a node ran against the state after it finished, so
/// only this node's incremental changes are reported.
pub(crate) fn compute_file_changes(
    baseline: &BTreeMap<String, Option<String>>,
    current: &BTreeMap<String, Option<String>>,
) -> Vec<FileChange> {
    let paths: BTreeSet<&String> = baseline.keys().chain(current.keys()).collect();
    let mut changes = Vec::new();
    for path in paths {
        let before = baseline.get(path).and_then(Clone::clone);
        let after = current.get(path).and_then(Clone::clone);
        let (additions, deletions) = match (before, after) {
            (None, Some(after)) => (count_lines(&after), 0),
            (Some(before), None) => (0, count_lines(&before)),
            (Some(before), Some(after)) => line_diff_counts(&before, &after),
            (None, None) => continue,
        };
        if additions > 0 || deletions > 0 {
            changes.push(FileChange {
                path: path.clone(),
                additions,
                deletions,
            });
        }
    }
    changes
}

/// Counts the added and removed lines between two file contents.
fn line_diff_counts(before: &str, after: &str) -> (u64, u64) {
    let diff = TextDiff::from_lines(before, after);
    let additions = diff
        .iter_all_changes()
        .filter(|change| change.tag() == ChangeTag::Insert)
        .count() as u64;
    let deletions = diff
        .iter_all_changes()
        .filter(|change| change.tag() == ChangeTag::Delete)
        .count() as u64;
    (additions, deletions)
}

/// Counts the lines of a file for new-file additions or whole-file deletions.
fn count_lines(content: &str) -> u64 {
    content.lines().count() as u64
}

/// Whether a node's stop should park an interactive node at `Pending` instead of reporting a
/// result. Completed and user-cancelled turns keep the conversation open for follow-up; a refusal
/// still fails the node.
fn pauses_interactive_node(interactive: bool, stop_reason: StopReason) -> bool {
    interactive
        && matches!(
            stop_reason,
            StopReason::EndTurn
                | StopReason::MaxTokens
                | StopReason::MaxTurnRequests
                | StopReason::Cancelled
        )
}

/// Persists the worktree snapshot captured when an interactive node started, so the completion
/// flow can later diff only this node's changes. Lives under a dedicated baselines root, never
/// the database or the worktree itself.
fn persist_worktree_baseline(
    baselines_root: &Path,
    node_run_id: &WorkflowNodeRunId,
    baseline: &BTreeMap<String, Option<String>>,
) -> Result<(), NodeExecutionError> {
    std::fs::create_dir_all(baselines_root).map_err(|source| {
        NodeExecutionError::BaselinePersist {
            node_id: node_run_id.to_string(),
            source,
        }
    })?;
    let path = baselines_root.join(format!("{}.json", node_run_id.as_ref()));
    let json =
        serde_json::to_vec(baseline).map_err(|error| NodeExecutionError::BaselinePersist {
            node_id: node_run_id.to_string(),
            source: error.into(),
        })?;
    std::fs::write(path, json).map_err(|source| NodeExecutionError::BaselinePersist {
        node_id: node_run_id.to_string(),
        source,
    })?;
    Ok(())
}

/// Runs the warm → attach → model → prompt → stop session chain for one agent node.
#[allow(clippy::too_many_arguments)]
async fn drive_agent_node(
    agent_runtime: &AgentRuntimeManager,
    pool: &RepositoryPool,
    skills_root: &Path,
    agent_repository: &SqliteAgentDefinitionRepository,
    clock: &SystemClock,
    baselines_root: &Path,
    node_run_id: &WorkflowNodeRunId,
    node: &WorkflowGraphNode,
    context: &ExecutionContext,
) -> Result<AgentNodeOutcome, NodeExecutionError> {
    let config =
        node.agent_config
            .as_ref()
            .ok_or_else(|| NodeExecutionError::MissingAgentConfig {
                node_id: node.id.clone(),
            })?;
    let agent_cli = resolve_agent_cli(&config.executor.agent_cli)?;

    // Warm a reusable provider session for this run's task.
    let warm = agent_runtime
        .warm_session_for_owner(
            WarmSessionRequest {
                target: WarmSessionTarget::Task {
                    task_id: context.task.id.to_string(),
                },
                agent_cli,
            },
            WarmOwner::WorkflowNode {
                run_id: context.run.id.to_string(),
                node_id: node.id.clone(),
            },
        )
        .await?;

    // Attach the warm session to the run task and bind it to the node run immediately, so the
    // workflow cancel path can always find and stop it while the first prompt is being prepared.
    let attach = agent_runtime
        .attach_session(AttachSessionRequest {
            session_id: warm.session_id.clone(),
            task_id: context.task.id.to_string(),
        })
        .await?;
    let repository = SqliteWorkflowRunEngineRepository::new(pool.clone());
    let now = clock.now_timestamp_millis();
    repository.set_node_run_session_id(node_run_id, &SessionId::new(attach.session.id), now)?;

    // Select the graph-declared model from the warm-advertised options; no silent fallback.
    let (config_id, model_value) = match_model_value(
        &warm.config_options,
        &config.executor.agent_cli,
        &config.executor.model_id,
    )?;
    agent_runtime
        .set_session_config(SetSessionConfigRequest {
            session_id: warm.session_id.clone(),
            config_id,
            value: model_value,
        })
        .await?;

    // Resolve the role's system instructions from the agents catalog; an empty role means no
    // system-instructions block is sent. Name is preferred; the id is a legacy fallback.
    let role_content = match &config.role_id {
        Some(role_id) if !role_id.trim().is_empty() => {
            let by_name =
                agent_repository.find_agent_definition_by_name(&Namespace::local(), role_id)?;
            let definition = if by_name.is_some() {
                by_name
            } else {
                agent_repository.find_agent_definition(&AgentDefinitionId::new(role_id))?
            };
            definition.map(|definition| definition.content)
        }
        _ => None,
    };

    // Assemble one explicit workflow handoff while preserving leading slash-command parsing.
    let node_runs = repository.list_node_runs(&context.run.id)?;
    let skill_names = resolve_skill_names(pool, skills_root, &config.skills)?;
    let prompt = assemble_workflow_prompt(WorkflowPromptRequest {
        node,
        role_content: role_content.as_deref(),
        graph_json: &context.graph_json,
        run_input: context.run.input.as_deref(),
        node_runs: &node_runs,
        skill_names: &skill_names,
        locale: workflow_prompt_locale(context.run.payload.as_deref()),
    });

    let worktree_root = agent_runtime.task_cwd(&context.task.id)?;

    // Snapshot the worktree before this node runs so its completion diff is the node's own
    // incremental change (previous nodes' changes are already in the baseline).
    let baseline = capture_worktree_snapshot(&worktree_root);

    let mut stream = agent_runtime
        .prompt_session(PromptSessionRequest {
            session_id: warm.session_id.clone(),
            prompt,
        })
        .await?;

    // Consume the owning prompt stream while `load_session` followers receive the same live turn.
    let mut accumulator = AssistantOutputAccumulator::default();
    let mut stop_reason = None;
    while let Some(event) = stream.recv().await {
        match event? {
            PromptSessionEvent::SessionUpdate { update } => {
                accumulator.consume(&update);
            }
            PromptSessionEvent::PermissionRequest(_) => {}
            PromptSessionEvent::Completed {
                stop_reason: reason,
            } => {
                stop_reason = Some(reason);
                break;
            }
        }
    }
    let stop_reason = stop_reason.ok_or(NodeExecutionError::SessionEndedWithoutStopReason)?;

    // An interactive node's first turn parks the node instead of completing it: the session stays
    // open for follow-up turns and the baseline is persisted for completion-time diffing.
    if pauses_interactive_node(config.interactive, stop_reason) {
        persist_worktree_baseline(baselines_root, node_run_id, &baseline)?;
        let now = clock.now_timestamp_millis();
        repository.transition_node_run_status(
            node_run_id,
            WorkflowNodeStatus::Running,
            WorkflowNodeStatus::Pending,
            now,
        )?;
        return Ok(AgentNodeOutcome::AwaitingInput);
    }

    // Stop the node's session; the Ora record stays queryable.
    agent_runtime
        .stop_session(StopSessionRequest {
            session_id: warm.session_id.clone(),
        })
        .await?;

    // Record the worktree delta since this node started: the baseline was captured before the
    // prompt, so only this node's own changes are reported, not earlier nodes' work.
    let file_changes = compute_file_changes(&baseline, &capture_worktree_snapshot(&worktree_root));

    Ok(AgentNodeOutcome::Completed {
        output: accumulator.into_output(),
        stop_reason,
        file_changes,
    })
}

/// Reads the locale frozen when the workflow run was created.
fn workflow_prompt_locale(payload: Option<&str>) -> WorkflowRunLocale {
    #[derive(Deserialize)]
    struct WorkflowRunPayload {
        locale: WorkflowRunLocale,
    }

    payload
        .and_then(|payload| serde_json::from_str::<WorkflowRunPayload>(payload).ok())
        .map(|payload| payload.locale)
        // Chinese is Ora's default locale and remains the safe fallback for damaged old data.
        .unwrap_or(WorkflowRunLocale::ZhCn)
}

/// Reports one finished turn to the engine according to the confirmed stop-reason mapping.
fn report_outcome(
    callback: &Arc<dyn WorkflowRunCallback>,
    run_id: &WorkflowRunId,
    node_run_id: &WorkflowNodeRunId,
    outcome: AgentNodeOutcome,
) {
    let AgentNodeOutcome::Completed {
        output,
        stop_reason,
        file_changes,
    } = outcome
    else {
        // An interactive node parked at `Pending` reports nothing; the human drives completion.
        return;
    };
    match stop_reason {
        StopReason::EndTurn => callback.complete_node(
            run_id,
            node_run_id,
            output,
            Some("end_turn".to_string()),
            file_changes,
        ),
        StopReason::MaxTokens => callback.complete_node(
            run_id,
            node_run_id,
            output,
            Some("max_tokens".to_string()),
            file_changes,
        ),
        StopReason::MaxTurnRequests => callback.complete_node(
            run_id,
            node_run_id,
            output,
            Some("max_turn_requests".to_string()),
            file_changes,
        ),
        StopReason::Refusal => callback.fail_node(
            run_id,
            node_run_id,
            "agent refused the request".to_string(),
            output,
        ),
        StopReason::Cancelled => {
            // Non-interactive cancellation belongs to the run cancel flow; interactive turns
            // already returned through the awaiting-input branch above.
        }
        // A newer ACP stop reason has semantics this executor cannot safely map to a
        // successful workflow transition.
        _ => callback.fail_node(
            run_id,
            node_run_id,
            "agent stopped for a reason this Ora version does not recognize".to_string(),
            output,
        ),
    }
}

/// Maps the graph's `agentCli` string to the transport `AgentCli` enum.
fn resolve_agent_cli(value: &str) -> Result<ContractAgentCli, NodeExecutionError> {
    match value {
        "open_code" => Ok(ContractAgentCli::OpenCode),
        "nga" => Ok(ContractAgentCli::Nga),
        "code_agent_cli" => Ok(ContractAgentCli::CodeAgentCli),
        "claude" => Ok(ContractAgentCli::Claude),
        "codex" => Ok(ContractAgentCli::Codex),
        _ => Err(NodeExecutionError::UnknownAgentCli {
            agent_cli: value.to_string(),
        }),
    }
}

/// Finds the model option and the value to select for the graph's `modelId`.
///
/// Matching follows the confirmed order: a `Model`-category select, falling back to the sole
/// select option; then an exact value match, then a label-contains match. No match fails the
/// node instead of silently using the CLI default.
fn match_model_value(
    config_options: &[SessionConfigOption],
    agent_cli: &str,
    model_id: &str,
) -> Result<(String, String), NodeExecutionError> {
    let model_option = config_options
        .iter()
        .find(|option| matches!(option.category, Some(SessionConfigOptionCategory::Model)))
        .or_else(|| {
            let selects: Vec<&SessionConfigOption> = config_options
                .iter()
                .filter(|option| matches!(option.kind, SessionConfigKind::Select(_)))
                .collect();
            (selects.len() == 1).then_some(selects[0])
        })
        .ok_or_else(|| NodeExecutionError::WorkflowModelNotFound {
            agent_cli: agent_cli.to_string(),
            model_id: model_id.to_string(),
        })?;
    let SessionConfigKind::Select(select) = &model_option.kind else {
        return Err(NodeExecutionError::WorkflowModelNotFound {
            agent_cli: agent_cli.to_string(),
            model_id: model_id.to_string(),
        });
    };
    let options: Vec<&SessionConfigSelectOption> = match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options.iter().collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .collect(),
        // New option container shapes require an explicit selection policy before
        // workflow execution can choose a model from them.
        _ => {
            return Err(NodeExecutionError::WorkflowModelNotFound {
                agent_cli: agent_cli.to_string(),
                model_id: model_id.to_string(),
            });
        }
    };
    let matched = options
        .iter()
        .find(|option| option.value.0.as_ref() == model_id || option.name.contains(model_id));
    match matched {
        Some(option) => Ok((model_option.id.0.to_string(), option.value.0.to_string())),
        None => Err(NodeExecutionError::WorkflowModelNotFound {
            agent_cli: agent_cli.to_string(),
            model_id: model_id.to_string(),
        }),
    }
}

/// Resolves each enabled skill to the executable `/name` its agent CLI uses, so the prompt can
/// invoke it explicitly instead of relying on the agent to discover the materialized package.
fn resolve_skill_names(
    pool: &RepositoryPool,
    skills_root: &Path,
    skills: &[AgentSkill],
) -> Result<Vec<String>, NodeExecutionError> {
    let storage = FilesystemSkillStorage::new(skills_root.to_path_buf());
    let skill_repository = SqliteSkillRepository::new(pool.clone());
    let mut names = Vec::new();
    for skill in skills.iter().filter(|skill| skill.enabled) {
        let name =
            resolve_executable_skill_name(&storage, Some(&skill_repository), &skill.skill_id)
                .map_err(|_| NodeExecutionError::SkillResolution {
                    skill_id: skill.skill_id.clone(),
                })?;
        names.push(name);
    }
    Ok(names)
}

/// Accumulates only the final assistant response produced by the node's prompt turn.
#[derive(Debug, Default)]
struct AssistantOutputAccumulator {
    text: String,
}

impl AssistantOutputAccumulator {
    /// Records assistant text while leaving the session history responsible for full conversation.
    fn consume(&mut self, update: &SessionUpdate) {
        if let SessionUpdate::AgentMessageChunk(chunk) = update
            && let Some(text) = chunk_text(&chunk.content)
        {
            self.text.push_str(text);
        }
    }

    /// Returns the assistant's scalar output, or `None` when no assistant text was produced.
    fn into_output(self) -> Option<String> {
        if self.text.is_empty() {
            return None;
        }
        Some(self.text)
    }
}

/// Extracts the text payload from a content block, ignoring non-text blocks.
fn chunk_text(block: &ContentBlock) -> Option<&str> {
    match block {
        ContentBlock::Text(text) => Some(&text.text),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol_schema::v1::{
        ContentChunk, SessionConfigId, SessionConfigSelect, SessionConfigValueId, TextContent,
    };
    use pretty_assertions::assert_eq;

    fn select_option(value: &str, name: &str) -> SessionConfigSelectOption {
        SessionConfigSelectOption::new(
            SessionConfigValueId::new(value.to_string()),
            name.to_string(),
        )
    }

    #[test]
    fn compute_file_changes_reports_only_the_incremental_delta() {
        let mut baseline = BTreeMap::new();
        baseline.insert("src/a.ts".to_string(), Some("one\ntwo\n".to_string()));
        baseline.insert("src/b.ts".to_string(), Some("keep\n".to_string()));

        let mut current = BTreeMap::new();
        current.insert(
            "src/a.ts".to_string(),
            Some("one\ntwo\nthree\n".to_string()),
        );
        current.insert("src/b.ts".to_string(), None);
        current.insert("src/new.ts".to_string(), Some("fresh\n".to_string()));

        // a.ts gained a line, b.ts was deleted, new.ts was added; keep unchanged is excluded.
        assert_eq!(
            compute_file_changes(&baseline, &current),
            vec![
                FileChange {
                    path: "src/a.ts".to_string(),
                    additions: 1,
                    deletions: 0
                },
                FileChange {
                    path: "src/b.ts".to_string(),
                    additions: 0,
                    deletions: 1
                },
                FileChange {
                    path: "src/new.ts".to_string(),
                    additions: 1,
                    deletions: 0
                },
            ]
        );
    }

    /// Verifies completed and user-cancelled turns park an interactive node.
    #[test]
    fn pauses_interactive_node_parks_completed_and_cancelled_turns() {
        assert!(pauses_interactive_node(true, StopReason::EndTurn));
        assert!(pauses_interactive_node(true, StopReason::MaxTokens));
        assert!(pauses_interactive_node(true, StopReason::MaxTurnRequests));
        // A refusal still fails the node, while an explicit prompt cancellation yields control.
        assert!(!pauses_interactive_node(true, StopReason::Refusal));
        assert!(pauses_interactive_node(true, StopReason::Cancelled));
        // Non-interactive nodes keep the existing complete-on-EndTurn behavior.
        assert!(!pauses_interactive_node(false, StopReason::EndTurn));
    }

    /// Verifies a persisted worktree baseline round-trips through its side file.
    #[test]
    fn persist_worktree_baseline_round_trips() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut baseline = BTreeMap::new();
        baseline.insert("src/a.ts".to_string(), Some("one\n".to_string()));
        baseline.insert("src/b.ts".to_string(), None);
        persist_worktree_baseline(temp.path(), &WorkflowNodeRunId::new("node-1"), &baseline)
            .unwrap();
        let loaded: BTreeMap<String, Option<String>> =
            serde_json::from_slice(&std::fs::read(temp.path().join("node-1.json")).unwrap())
                .unwrap();
        assert_eq!(loaded, baseline);
    }

    /// Verifies the completion-time diff against a baseline loaded from its side file reports
    /// only the node's own changes since the node started.
    #[test]
    fn compute_file_changes_against_a_loaded_baseline() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut baseline = BTreeMap::new();
        baseline.insert("src/a.ts".to_string(), Some("one\n".to_string()));
        persist_worktree_baseline(temp.path(), &WorkflowNodeRunId::new("node-1"), &baseline)
            .unwrap();
        let loaded: BTreeMap<String, Option<String>> =
            serde_json::from_slice(&std::fs::read(temp.path().join("node-1.json")).unwrap())
                .unwrap();

        let mut current = baseline;
        current.insert("src/a.ts".to_string(), Some("one\ntwo\n".to_string()));
        current.insert("src/new.ts".to_string(), Some("fresh\n".to_string()));
        assert_eq!(
            compute_file_changes(&loaded, &current),
            vec![
                FileChange {
                    path: "src/a.ts".to_string(),
                    additions: 1,
                    deletions: 0
                },
                FileChange {
                    path: "src/new.ts".to_string(),
                    additions: 1,
                    deletions: 0
                },
            ]
        );
    }

    /// Verifies the snapshot covers clean tracked files and files inside untracked directories,
    /// so the before/after delta reports the node's own edits rather than whole-file additions.
    #[test]
    fn capture_worktree_snapshot_diffs_clean_tracked_and_untracked_dir_files() {
        let scaffold = ora_test_support::GitTestScaffold::new("backend-workflow-snapshot")
            .expect("create Git test scaffold");
        let root = scaffold.repo_path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        // A tracked file that is clean at the baseline and modified by the node.
        std::fs::write(root.join("src/a.ts"), "one\ntwo\n").unwrap();
        scaffold
            .stage_all_and_commit("init")
            .expect("create snapshot baseline commit");

        let baseline = capture_worktree_snapshot(root);
        // The clean tracked file is part of the baseline.
        assert_eq!(
            baseline.get("src/a.ts"),
            Some(&Some("one\ntwo\n".to_string()))
        );

        // The node edits the tracked file and creates files inside a new untracked directory.
        std::fs::write(root.join("src/a.ts"), "one\ntwo\nthree\n").unwrap();
        std::fs::create_dir_all(root.join("openspec/changes/demo")).unwrap();
        std::fs::write(root.join("openspec/changes/demo/proposal.md"), "fresh\n").unwrap();

        assert_eq!(
            compute_file_changes(&baseline, &capture_worktree_snapshot(root)),
            vec![
                FileChange {
                    path: "openspec/changes/demo/proposal.md".to_string(),
                    additions: 1,
                    deletions: 0
                },
                FileChange {
                    path: "src/a.ts".to_string(),
                    additions: 1,
                    deletions: 0
                },
            ]
        );
    }

    fn model_option(options: Vec<SessionConfigSelectOption>) -> SessionConfigOption {
        SessionConfigOption::new(
            SessionConfigId::new("model".to_string()),
            "Model",
            SessionConfigKind::Select(SessionConfigSelect::new(
                SessionConfigValueId::new("current".to_string()),
                SessionConfigSelectOptions::Ungrouped(options),
            )),
        )
        .category(SessionConfigOptionCategory::Model)
    }

    #[test]
    fn resolve_agent_cli_maps_snake_case_names() {
        assert_eq!(
            resolve_agent_cli("open_code").unwrap(),
            ContractAgentCli::OpenCode
        );
        assert_eq!(resolve_agent_cli("codex").unwrap(), ContractAgentCli::Codex);
        assert!(matches!(
            resolve_agent_cli("bogus"),
            Err(NodeExecutionError::UnknownAgentCli { .. })
        ));
    }

    #[test]
    fn match_model_value_prefers_exact_value_then_label() {
        let options = vec![
            select_option("fast", "Fast model"),
            select_option("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro"),
        ];
        let config = vec![model_option(options)];
        assert_eq!(
            match_model_value(&config, "open_code", "deepseek/deepseek-v4-pro").unwrap(),
            ("model".to_string(), "deepseek/deepseek-v4-pro".to_string())
        );
        // A label-contains match also works (case-sensitive, per the confirmed model rule).
        assert_eq!(
            match_model_value(&config, "open_code", "DeepSeek").unwrap(),
            ("model".to_string(), "deepseek/deepseek-v4-pro".to_string())
        );
    }

    #[test]
    fn match_model_value_fails_when_no_option_matches() {
        let config = vec![model_option(vec![select_option("fast", "Fast model")])];
        assert!(matches!(
            match_model_value(&config, "open_code", "missing-model"),
            Err(NodeExecutionError::WorkflowModelNotFound { .. })
        ));
    }

    #[test]
    fn match_model_value_falls_back_to_the_lone_select() {
        let option = SessionConfigOption::new(
            SessionConfigId::new("model".to_string()),
            "Model",
            SessionConfigKind::Select(SessionConfigSelect::new(
                SessionConfigValueId::new("smart".to_string()),
                SessionConfigSelectOptions::Ungrouped(vec![select_option("smart", "Smart")]),
            )),
        );
        assert_eq!(
            match_model_value(&[option], "open_code", "smart").unwrap(),
            ("model".to_string(), "smart".to_string())
        );
    }

    #[test]
    fn assistant_output_accumulator_keeps_only_assistant_text() {
        let mut accumulator = AssistantOutputAccumulator::default();
        accumulator.consume(&SessionUpdate::UserMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("ignored")),
        )));
        accumulator.consume(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("hello ")),
        )));
        accumulator.consume(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("world")),
        )));
        assert_eq!(accumulator.into_output(), Some("hello world".to_string()));
    }

    #[test]
    fn workflow_prompt_locale_reads_the_locale_frozen_on_the_run() {
        assert_eq!(
            workflow_prompt_locale(Some(r#"{"locale":"en-US"}"#)),
            WorkflowRunLocale::EnUs
        );
        assert_eq!(workflow_prompt_locale(None), WorkflowRunLocale::ZhCn);
    }
}
