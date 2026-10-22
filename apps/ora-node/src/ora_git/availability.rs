//! Session-facing availability: disabling Git access must not disable ordinary Agent sessions.

use super::{OraGitBroker, OraGitSocket, SessionGit};
use std::{collections::BTreeMap, io, sync::Arc};

/// A configured service either has an isolated workload identity or explains why it is disabled.
pub(crate) enum Availability {
    Disabled(String),
    Enabled(Arc<dyn OraGitBroker>),
}

impl Availability {
    /// Refuses both ways a workload can share the Node identity, before any service is created.
    pub(crate) fn for_workload(
        workload_uid: Option<u32>,
        node_uid: u32,
        enable: impl FnOnce(u32) -> Result<Arc<dyn OraGitBroker>, crate::Error>,
    ) -> Result<Self, crate::Error> {
        let reason = match workload_uid {
            None => "ora-git is disabled: process.workload_uid is not set; it must differ from the Node effective UID".to_owned(),
            Some(uid) if uid == node_uid => format!("ora-git is disabled: process.workload_uid ({uid}) equals the Node effective UID; configure a separate workload UID"),
            Some(uid) => return enable(uid).map(Self::Enabled),
        };
        ora_logging::ora_warn!(reason = %reason, "ora-git disabled");
        Ok(Self::Disabled(reason))
    }

    /// Publishes either the live socket or the refusal reason to the Agent's process environment.
    pub(crate) fn open(
        &self,
        session: SessionGit,
        environment: &mut BTreeMap<String, String>,
    ) -> io::Result<Option<OraGitSocket>> {
        match self {
            Self::Disabled(reason) => {
                environment.insert(ora_git::DISABLED_ENV.to_owned(), reason.clone());
                Ok(None)
            }
            Self::Enabled(broker) => {
                let socket = broker.open(session)?;
                environment.insert(
                    ora_git::SOCKET_ENV.to_owned(),
                    socket.path().to_string_lossy().into_owned(),
                );
                Ok(Some(socket))
            }
        }
    }
}
