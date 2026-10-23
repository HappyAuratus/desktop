//! What a scheduled session declares about its business result at a normal end: the verdict its
//! agent stated in the final Assistant turn, and the artifacts it left under the reserved
//! directory.
//!
//! Both are derived from what the run already produced — the sealed history and the checkout — and
//! neither writes to the Workspace. Cloud admits scheduled occurrences but never parses a Thread,
//! a message or a tool field, so this module is the only place a run's business result comes from
//! Agent output. A manual run, and every failed, cancelled or interrupted end, declares nothing.

mod artifacts;
#[cfg(test)]
mod tests;

use super::end::SessionEnd;
use ora_domain::parse_scheduled_run_verdict;
use ora_node_protocol::{
    AgentSessionEndReason, ExecutionId, ScheduledArtifactManifest, ScheduledRunResult,
    ScheduledVerdict, SessionTrigger,
};
use std::path::Path;

/// What one scheduled run reports besides its execution result.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ScheduledDeclaration {
    /// The verdict, absent when the agent stated none or stated an unusable one.
    pub(super) result: Option<ScheduledRunResult>,
    /// The declared artifacts, absent when the run had no artifact directory at all.
    pub(super) manifest: Option<ScheduledArtifactManifest>,
}

/// Derives the declaration of a session that is about to settle, or `None` when it declares
/// nothing.
///
/// Only a scheduled occurrence declares, and only when Cloud ended it as asked: a cancelled,
/// failed or interrupted session has no completed turn whose conclusion could describe the run.
pub(super) fn declare(
    end: &SessionEnd,
    trigger: Option<SessionTrigger>,
    sessions_root: &Path,
    execution: &ExecutionId,
    checkout: &Path,
) -> Option<ScheduledDeclaration> {
    if trigger != Some(SessionTrigger::Scheduled) {
        return None;
    }
    if !matches!(
        end.reason,
        AgentSessionEndReason::UserEnded | AgentSessionEndReason::IdleTimeout
    ) {
        return None;
    }
    let declaration = ScheduledDeclaration {
        result: parse_verdict(sessions_root, execution),
        manifest: artifacts::collect(checkout),
    };
    (declaration.result.is_some() || declaration.manifest.is_some()).then_some(declaration)
}

/// Reads the run's final Assistant turn and parses the documented sentinel out of it.
///
/// Any failure — unreadable history, no assistant turn, no marker, an illegal value or an
/// over-long detail — means "not reported"; the run's execution result is never rewritten for it.
fn parse_verdict(sessions_root: &Path, execution: &ExecutionId) -> Option<ScheduledRunResult> {
    let history = ora_history::read_session_history(sessions_root, execution.as_str()).ok()?;
    let text = ora_history::final_assistant_message(&history)?;
    let annotation = parse_scheduled_run_verdict(&text)?;
    Some(ScheduledRunResult {
        verdict: verdict(annotation.verdict),
        verdict_detail: annotation.detail,
    })
}

/// Maps the shared vocabulary onto its wire spelling; both sides list the same six conclusions.
fn verdict(verdict: ora_domain::ScheduledRunVerdict) -> ScheduledVerdict {
    match verdict {
        ora_domain::ScheduledRunVerdict::Pass => ScheduledVerdict::Pass,
        ora_domain::ScheduledRunVerdict::Warn => ScheduledVerdict::Warn,
        ora_domain::ScheduledRunVerdict::Fail => ScheduledVerdict::Fail,
        ora_domain::ScheduledRunVerdict::Blocked => ScheduledVerdict::Blocked,
        ora_domain::ScheduledRunVerdict::Idle => ScheduledVerdict::Idle,
        ora_domain::ScheduledRunVerdict::RangeUnresolved => ScheduledVerdict::RangeUnresolved,
    }
}
