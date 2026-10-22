//! Domain-to-contract mapping for the scheduled-run terminal state.
//!
//! These pure functions translate the run's domain enums into the wire-facing
//! `ora_contracts` representations. They are kept separate from the runtime so the
//! settlement responsibility stays small and the mapping is easy to unit-test.

use agent_client_protocol_schema::v1::StopReason;
use ora_domain::{ScheduledRunFailureReason, ScheduledRunStatus, ScheduledRunTransition};

/// Maps the session stop reason to the run's terminal transition.
pub(super) fn scheduled_run_completion_transition(
    stop_reason: StopReason,
) -> ScheduledRunTransition {
    match stop_reason {
        StopReason::EndTurn => ScheduledRunTransition::Succeed,
        StopReason::Cancelled => ScheduledRunTransition::Cancel,
        StopReason::MaxTokens | StopReason::MaxTurnRequests | StopReason::Refusal => {
            ScheduledRunTransition::Fail {
                reason: ScheduledRunFailureReason::ExecutionError,
            }
        }
        _ => ScheduledRunTransition::Fail {
            reason: ScheduledRunFailureReason::ExecutionError,
        },
    }
}

/// Maps the domain run status to its contract representation.
pub(super) fn contract_run_status(status: ScheduledRunStatus) -> ora_contracts::ScheduledRunStatus {
    match status {
        ScheduledRunStatus::Accepted => ora_contracts::ScheduledRunStatus::Accepted,
        ScheduledRunStatus::Running => ora_contracts::ScheduledRunStatus::Running,
        ScheduledRunStatus::Succeeded => ora_contracts::ScheduledRunStatus::Succeeded,
        ScheduledRunStatus::Failed => ora_contracts::ScheduledRunStatus::Failed,
        ScheduledRunStatus::Cancelled => ora_contracts::ScheduledRunStatus::Cancelled,
        ScheduledRunStatus::SkippedBusy => ora_contracts::ScheduledRunStatus::SkippedBusy,
        ScheduledRunStatus::SkippedCapacity => ora_contracts::ScheduledRunStatus::SkippedCapacity,
    }
}

/// Maps the domain failure reason to its contract representation.
pub(super) fn contract_failure_reason(
    reason: ScheduledRunFailureReason,
) -> ora_contracts::ScheduledRunFailureReason {
    match reason {
        ScheduledRunFailureReason::TargetUnavailable => {
            ora_contracts::ScheduledRunFailureReason::TargetUnavailable
        }
        ScheduledRunFailureReason::ModelUnavailable => {
            ora_contracts::ScheduledRunFailureReason::ModelUnavailable
        }
        ScheduledRunFailureReason::WorktreeFailed => {
            ora_contracts::ScheduledRunFailureReason::WorktreeFailed
        }
        ScheduledRunFailureReason::ApprovalPolicyInvalid => {
            ora_contracts::ScheduledRunFailureReason::ApprovalPolicyInvalid
        }
        ScheduledRunFailureReason::Timeout => ora_contracts::ScheduledRunFailureReason::Timeout,
        ScheduledRunFailureReason::ExecutionError => {
            ora_contracts::ScheduledRunFailureReason::ExecutionError
        }
    }
}

/// Maps the domain miss reason to its contract representation.
pub(super) fn contract_miss_reason(
    reason: ora_domain::ScheduledMissReason,
) -> ora_contracts::ScheduledMissReason {
    match reason {
        ora_domain::ScheduledMissReason::AppNotRunning => {
            ora_contracts::ScheduledMissReason::AppNotRunning
        }
        ora_domain::ScheduledMissReason::LateTrigger => {
            ora_contracts::ScheduledMissReason::LateTrigger
        }
    }
}
