use crate::ProjectId;
use serde::{Deserialize, Serialize};

/// Represents a metadata-driven folder mounted into a project workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualFolder {
    pub id: crate::VirtualFolderId,
    pub project_id: ProjectId,
    pub name: String,
    pub mount_point: String,
}

impl VirtualFolder {
    pub fn new(
        id: crate::VirtualFolderId,
        project_id: ProjectId,
        name: impl Into<String>,
        mount_point: impl Into<String>,
    ) -> Self {
        Self {
            id,
            project_id,
            name: name.into(),
            mount_point: mount_point.into(),
        }
    }
}
