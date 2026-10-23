//! What a scheduled session declares about its business result at a normal end: the verdict its
//! agent stated in the final Assistant turn, and the artifacts it left under the reserved
//! directory.
//!
//! Both are derived from what the run already produced — the sealed history and the checkout — and
//! neither writes to the Workspace. Cloud admits scheduled occurrences but never parses a Thread,
//! a message or a tool field, so this module is the only place a run's business result comes from
//! Agent output. A manual run, every failed, cancelled or interrupted end, and a run whose final
//! turn the end cancelled or left unfinished declare nothing.

mod artifacts;
#[cfg(test)]
mod tests;

use super::end::SessionEnd;
use ora_domain::parse_scheduled_run_verdict;
use ora_logging::ora_warn;
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
/// Only a scheduled occurrence declares, and only when Cloud ended it as asked after the agent
/// finished a turn: a cancelled, failed or interrupted session has no completed turn whose
/// conclusion could describe the run. The completed turn gates both facts, because an end command
/// can cancel the turn that was still running while the session still ends as asked. A completed
/// turn that said nothing still gates the artifacts, because a turn may finish its work through
/// tools alone; only its verdict is then unreported. Each fact is validated against the wire rules
/// before it is declared, so nothing this module produces can make the terminal write fail.
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
    let final_turn = completed_final_turn(sessions_root, execution)?;
    // The completed turn proves the agent finished its work; what it said there is optional. A turn
    // that only used tools has no verdict text, but the artifacts it wrote are still the run's
    // declaration, so only the verdict stays unreported.
    let declaration = ScheduledDeclaration {
        result: parse_verdict(&final_turn, execution),
        manifest: reportable_manifest(checkout, execution),
    };
    (declaration.result.is_some() || declaration.manifest.is_some()).then_some(declaration)
}

/// Reads the final Assistant turn, present only when `TurnEnded` closed it normally.
///
/// An end command can cancel the turn that is running, so the end reason alone does not prove the
/// agent completed its work. An unreadable history, a cancelled or truncated turn, and a turn that
/// never ended all mean "cannot tell", and an earlier turn is never used instead. A completed turn
/// is evidence even when it carries no Assistant text, so its empty text is returned rather than
/// mistaken for a missing turn.
fn completed_final_turn(sessions_root: &Path, execution: &ExecutionId) -> Option<String> {
    let history = ora_history::read_session_history(sessions_root, execution.as_str()).ok()?;
    ora_history::final_completed_assistant_turn(&history)
}

/// Parses the documented sentinel out of the run's final completed Assistant turn.
///
/// Any failure — no marker, an illegal value or a detail the wire would refuse — means "not
/// reported"; the run's execution result is never rewritten for it.
fn parse_verdict(text: &str, execution: &ExecutionId) -> Option<ScheduledRunResult> {
    let annotation = parse_scheduled_run_verdict(text)?;
    let result = ScheduledRunResult {
        verdict: verdict(annotation.verdict),
        verdict_detail: annotation.detail,
    };
    if result.validate().is_err() {
        // The wire refuses some details the parser can still read (a NUL, say). Reporting one would
        // fail the terminal transaction and lose the run's own settlement, so it degrades to
        // "not reported" exactly like an illegal value. Artifacts stay independently reported.
        ora_warn!(
            execution_id = %execution,
            "a scheduled verdict detail was not reportable and was left unreported"
        );
        return None;
    }
    Some(result)
}

/// Collects the artifact directory and keeps the declaration only if the wire accepts it.
fn reportable_manifest(
    checkout: &Path,
    execution: &ExecutionId,
) -> Option<ScheduledArtifactManifest> {
    let manifest = artifacts::collect(checkout)?;
    match manifest.validate() {
        Ok(()) => Some(manifest),
        Err(error) => {
            ora_warn!(
                execution_id = %execution,
                error = %error,
                "a scheduled artifact declaration was not reportable and was left unreported"
            );
            None
        }
    }
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
