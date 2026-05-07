use crate::{DomainModelError, TaskId};
use serde::{Deserialize, Serialize};

/// Models whether a worktree is the active working copy for its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorktreeActivity {
    Inactive,
    Active,
}

impl WorktreeActivity {
    /// Returns the integer code used by persistence adapters for this activity value.
    pub fn database_value(self) -> i64 {
        match self {
            Self::Inactive => 0,
            Self::Active => 1,
        }
    }

    /// Converts a persisted integer into a strongly typed worktree activity value.
    pub fn from_database_value(value: i64) -> Result<Self, DomainModelError> {
        match value {
            0 => Ok(Self::Inactive),
            1 => Ok(Self::Active),
            _ => Err(DomainModelError::InvalidWorktreeActivity(value)),
        }
    }
}

impl TryFrom<i64> for WorktreeActivity {
    type Error = DomainModelError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::from_database_value(value)
    }
}

/// Represents the physical git worktree that backs a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worktree {
    pub id: crate::WorktreeId,
    pub task_id: TaskId,
    pub branch_name: Option<String>,
    pub activity: WorktreeActivity,
}

impl Worktree {
    pub fn new(
        id: crate::WorktreeId,
        task_id: TaskId,
        branch_name: Option<String>,
        activity: WorktreeActivity,
    ) -> Self {
        Self {
            id,
            task_id,
            branch_name,
            activity,
        }
    }
}
