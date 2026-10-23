#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::artifacts;
use super::declare;
use crate::session::end::SessionEnd;
use agent_client_protocol_schema::v1::{
    ContentBlock, ContentChunk, SessionUpdate, StopReason, TextContent,
};
use ora_domain::{MAX_SCHEDULED_ARTIFACT_BYTES, MAX_SCHEDULED_ARTIFACTS_PER_RUN};
use ora_history::{HistoryLine, HistoryRecord, history_path};
use ora_node_protocol::{
    EndSessionReason, ExecutionId, SCHEDULED_ARTIFACT_DIRECTORY, ScheduledArtifactItem,
    ScheduledArtifactKind, ScheduledArtifactRejection, ScheduledArtifactRejectionReason,
    ScheduledVerdict, SessionTrigger,
};
use pretty_assertions::assert_eq;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Builds the checkout of one scheduled run.
fn checkout() -> TempDir {
    TempDir::new().expect("a checkout directory")
}

/// Returns the reserved artifact directory inside one checkout, creating it.
fn artifact_directory(checkout: &Path) -> PathBuf {
    let directory = checkout.join(SCHEDULED_ARTIFACT_DIRECTORY);
    std::fs::create_dir_all(&directory).expect("the artifact directory");
    directory
}

/// Writes one session's history as the sealed JSONL the Node leaves behind.
fn write_history(root: &Path, execution: &ExecutionId, records: Vec<HistoryRecord>) {
    let path = history_path(root, execution.as_str()).expect("a history path");
    std::fs::create_dir_all(path.parent().expect("a history parent")).expect("the history tree");
    let mut file = String::new();
    for (index, record) in records.into_iter().enumerate() {
        let line = HistoryLine::new("2026-10-23T10:00:00+08:00", index as u32, record);
        file.push_str(&serde_json::to_string(&line).expect("a history line"));
        file.push('\n');
    }
    std::fs::write(&path, file).expect("the history file");
}

/// Builds one settled Assistant message.
fn assistant(text: &str) -> HistoryRecord {
    HistoryRecord::Update {
        update: Box::new(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(text)),
        ))),
        tool_timing: None,
    }
}

/// Closes one turn, which is what makes the following messages a later Assistant turn.
fn turn_ended() -> HistoryRecord {
    HistoryRecord::TurnEnded {
        stop_reason: StopReason::EndTurn,
    }
}

/// Builds one declared item for the artifact of `text` at `name`.
fn item(index: u32, name: &str, kind: ScheduledArtifactKind, text: &str) -> ScheduledArtifactItem {
    ScheduledArtifactItem {
        index,
        display_name: name.to_string(),
        kind,
        size_bytes: text.len() as u64,
        sha256: ora_utils::hash::sha256_hex(text.as_bytes()),
    }
}

#[test]
fn a_manual_run_declares_nothing() {
    let root = checkout();
    let execution = ExecutionId::new("execution-manual");
    write_history(root.path(), &execution, vec![assistant("QA_VERDICT: fail")]);

    assert_eq!(
        declare(
            &SessionEnd::requested(EndSessionReason::IdleTimeout),
            None,
            root.path(),
            &execution,
            root.path(),
        ),
        None
    );
}

#[test]
fn a_scheduled_run_that_did_not_end_normally_declares_nothing() {
    let root = checkout();
    let execution = ExecutionId::new("execution-cancelled");
    write_history(root.path(), &execution, vec![assistant("QA_VERDICT: fail")]);
    artifact_directory(root.path());

    for end in [
        SessionEnd::requested(EndSessionReason::Cancelled),
        SessionEnd::agent_failed("agent_plugin_unavailable"),
    ] {
        assert_eq!(
            declare(
                &end,
                Some(SessionTrigger::Scheduled),
                root.path(),
                &execution,
                root.path(),
            ),
            None
        );
    }
}

#[test]
fn declares_the_verdict_of_the_final_assistant_turn() {
    let root = checkout();
    let execution = ExecutionId::new("execution-verdict");
    write_history(
        root.path(),
        &execution,
        vec![
            assistant("QA_VERDICT: pass"),
            turn_ended(),
            assistant(
                "Reviewed every changed file.\nQA_VERDICT: fail\nQA_DETAIL: two critical defects",
            ),
            turn_ended(),
        ],
    );

    let declaration = declare(
        &SessionEnd::requested(EndSessionReason::IdleTimeout),
        Some(SessionTrigger::Scheduled),
        root.path(),
        &execution,
        root.path(),
    )
    .expect("a declaration");

    assert_eq!(
        declaration.result,
        Some(ora_node_protocol::ScheduledRunResult {
            verdict: ScheduledVerdict::Fail,
            verdict_detail: Some("two critical defects".to_string()),
        })
    );
}

#[test]
fn an_unusable_marker_reports_no_verdict() {
    let root = checkout();
    let execution = ExecutionId::new("execution-illegal");
    write_history(
        root.path(),
        &execution,
        vec![assistant("QA_VERDICT: banana"), turn_ended()],
    );

    let declaration = declare(
        &SessionEnd::requested(EndSessionReason::UserEnded),
        Some(SessionTrigger::Scheduled),
        root.path(),
        &execution,
        root.path(),
    );

    assert_eq!(declaration, None);
}

#[test]
fn a_run_without_artifacts_or_a_verdict_declares_nothing() {
    let root = checkout();
    let execution = ExecutionId::new("execution-quiet");
    write_history(
        root.path(),
        &execution,
        vec![assistant("nothing to report")],
    );

    assert_eq!(
        declare(
            &SessionEnd::requested(EndSessionReason::IdleTimeout),
            Some(SessionTrigger::Scheduled),
            root.path(),
            &execution,
            root.path(),
        ),
        None
    );
}

#[test]
fn collects_whitelisted_files_recursively_with_their_sizes_and_digests() {
    let checkout = checkout();
    let directory = artifact_directory(checkout.path());
    std::fs::create_dir_all(directory.join("nested")).expect("a nested directory");
    std::fs::write(directory.join("report.md"), "hello").expect("a report");
    std::fs::write(directory.join("nested/result.json"), "{}").expect("a result");

    assert_eq!(
        artifacts::collect(checkout.path()),
        Some(ora_node_protocol::ScheduledArtifactManifest {
            // The walk sorts each directory level, so the nested file comes first.
            items: vec![
                item(0, "nested/result.json", ScheduledArtifactKind::Json, "{}"),
                item(1, "report.md", ScheduledArtifactKind::Markdown, "hello"),
            ],
            rejections: Vec::new(),
        })
    );
}

#[test]
fn a_run_without_the_artifact_directory_declares_no_manifest() {
    assert_eq!(artifacts::collect(checkout().path()), None);
}

#[test]
fn refuses_a_candidate_outside_the_type_whitelist() {
    let checkout = checkout();
    let directory = artifact_directory(checkout.path());
    std::fs::write(directory.join("report.pdf"), "pdf").expect("a refused report");

    assert_eq!(
        artifacts::collect(checkout.path()),
        Some(ora_node_protocol::ScheduledArtifactManifest {
            items: Vec::new(),
            rejections: vec![ScheduledArtifactRejection {
                display_name: "report.pdf".to_string(),
                reason: ScheduledArtifactRejectionReason::UnsupportedType,
            }],
        })
    );
}

#[test]
fn refuses_a_candidate_larger_than_the_single_artifact_bound() {
    let checkout = checkout();
    let directory = artifact_directory(checkout.path());
    let oversized = vec![b'a'; MAX_SCHEDULED_ARTIFACT_BYTES as usize + 1];
    std::fs::write(directory.join("huge.log"), oversized).expect("an oversized log");

    assert_eq!(
        artifacts::collect(checkout.path()),
        Some(ora_node_protocol::ScheduledArtifactManifest {
            items: Vec::new(),
            rejections: vec![ScheduledArtifactRejection {
                display_name: "huge.log".to_string(),
                reason: ScheduledArtifactRejectionReason::TooLarge,
            }],
        })
    );
}

#[test]
fn declares_at_most_the_per_run_artifact_count() {
    let checkout = checkout();
    let directory = artifact_directory(checkout.path());
    for index in 0..=MAX_SCHEDULED_ARTIFACTS_PER_RUN {
        std::fs::write(directory.join(format!("report-{index:02}.txt")), "x").expect("a report");
    }

    let manifest = artifacts::collect(checkout.path()).expect("a manifest");

    assert_eq!(manifest.items.len(), MAX_SCHEDULED_ARTIFACTS_PER_RUN);
    assert_eq!(
        manifest.rejections,
        vec![ScheduledArtifactRejection {
            display_name: format!("report-{MAX_SCHEDULED_ARTIFACTS_PER_RUN:02}.txt"),
            reason: ScheduledArtifactRejectionReason::TooMany,
        }]
    );
}

#[cfg(unix)]
#[test]
fn refuses_a_symbolic_link_out_of_the_artifact_directory() {
    let checkout = checkout();
    let directory = artifact_directory(checkout.path());
    let outside = checkout.path().join("outside.md");
    std::fs::write(&outside, "QA_VERDICT: pass").expect("a file outside the directory");
    std::os::unix::fs::symlink(&outside, directory.join("link.md")).expect("a symbolic link");

    assert_eq!(
        artifacts::collect(checkout.path()),
        Some(ora_node_protocol::ScheduledArtifactManifest {
            items: Vec::new(),
            rejections: vec![ScheduledArtifactRejection {
                display_name: "link.md".to_string(),
                reason: ScheduledArtifactRejectionReason::Symlink,
            }],
        })
    );
}

#[cfg(unix)]
#[test]
fn refuses_a_hard_link_whose_inode_is_reachable_elsewhere() {
    let checkout = checkout();
    let directory = artifact_directory(checkout.path());
    let outside = checkout.path().join("outside.log");
    std::fs::write(&outside, "log").expect("a file outside the directory");
    std::fs::hard_link(&outside, directory.join("linked.log")).expect("a hard link");

    assert_eq!(
        artifacts::collect(checkout.path()),
        Some(ora_node_protocol::ScheduledArtifactManifest {
            items: Vec::new(),
            rejections: vec![ScheduledArtifactRejection {
                display_name: "linked.log".to_string(),
                reason: ScheduledArtifactRejectionReason::HardLink,
            }],
        })
    );
}

#[cfg(unix)]
#[test]
fn refuses_a_symlinked_artifact_directory_whole() {
    let checkout = checkout();
    let elsewhere = checkout.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("a directory elsewhere");
    std::fs::write(elsewhere.join("report.md"), "hello").expect("a report elsewhere");
    std::os::unix::fs::symlink(
        &elsewhere,
        checkout.path().join(SCHEDULED_ARTIFACT_DIRECTORY),
    )
    .expect("a symbolic link");

    assert_eq!(
        artifacts::collect(checkout.path()),
        Some(ora_node_protocol::ScheduledArtifactManifest {
            items: Vec::new(),
            rejections: vec![ScheduledArtifactRejection {
                display_name: SCHEDULED_ARTIFACT_DIRECTORY.to_string(),
                reason: ScheduledArtifactRejectionReason::Symlink,
            }],
        })
    );
}
