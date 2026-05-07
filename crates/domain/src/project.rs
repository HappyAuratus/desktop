use crate::ProjectId;
use serde::{Deserialize, Serialize};

/// Represents a top-level Ora project rooted at a physical workspace path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub root_path: String,
}

impl Project {
    pub fn new(id: ProjectId, name: impl Into<String>, root_path: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            root_path: root_path.into(),
        }
    }
}
