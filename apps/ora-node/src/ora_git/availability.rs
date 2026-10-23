//! Session-facing availability: disabling Git access must not disable ordinary Agent sessions.

use super::{OraGitBroker, OraGitSocket, SessionGit};
use std::{collections::BTreeMap, fmt, io, sync::Arc};

/// The configuration prevents distinguishing a workload from the Node by peer UID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisabledReason {
    /// No separate workload identity was configured.
    Unset,
    /// The configured workload identity equals the Node's effective UID.
    SharesNodeUid(u32),
}

impl fmt::Display for DisabledReason {
    /// Explains the configuration change needed to enable the service.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => formatter.write_str("ora-git is disabled: process.workload_uid is not set; it must differ from the Node effective UID"),
            Self::SharesNodeUid(uid) => write!(formatter, "ora-git is disabled: process.workload_uid ({uid}) equals the Node effective UID; configure a separate workload UID"),
        }
    }
}

/// A configured service either has an isolated workload identity or explains why it is disabled.
pub(crate) enum Availability {
    Disabled(DisabledReason),
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
            None => DisabledReason::Unset,
            Some(uid) if uid == node_uid => DisabledReason::SharesNodeUid(uid),
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
                environment.insert(ora_git::DISABLED_ENV.to_owned(), reason.to_string());
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
