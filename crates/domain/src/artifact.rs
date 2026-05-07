use crate::{ArtifactId, TaskId};
use serde::{Deserialize, Serialize};

/// Represents a persisted artifact that belongs to a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: ArtifactId,
    pub task_id: TaskId,
    pub content: Option<String>,
}

impl Artifact {
    pub fn new(id: ArtifactId, task_id: TaskId, content: Option<String>) -> Self {
        Self {
            id,
            task_id,
            content,
        }
    }
}
