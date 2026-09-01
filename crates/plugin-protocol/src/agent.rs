use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

/// Method that starts the agent process owned by a plugin.
pub const AGENT_START_METHOD: &str = "agent/start";
/// Method that stops the agent while leaving its plugin process alive.
pub const AGENT_STOP_METHOD: &str = "agent/stop";
/// Method that lists models before an ACP session exists.
pub const AGENT_LIST_MODELS_METHOD: &str = "agent/list_models";
/// Bidirectional notification that carries one opaque ACP frame.
pub const AGENT_ACP_METHOD: &str = "agent/acp";

/// Error returned when the agent executable is not installed on the machine.
pub const AGENT_NOT_INSTALLED_CODE: i64 = -32001;
/// Error returned when the executable bundled by an agent package cannot run.
pub const AGENT_UNUSABLE_CODE: i64 = -32002;
/// ACP major version carried over the agent plugin channel.
pub const SUPPORTED_ACP_VERSION: u32 = 1;

/// Host context handed to an agent when its underlying process starts.
#[derive(Debug, Clone, Deserialize, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agent.ts")]
pub struct AgentStartContext {
    #[ts(type = "string")]
    pub cwd: PathBuf,
    pub host_version: String,
}

/// Wire protocol used inside the bidirectional `agent/acp` notification.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "agent.ts")]
pub enum AgentProtocol {
    Acp,
}

/// Confirmation that a started agent is ready to receive ACP frames.
#[derive(Debug, Clone, Deserialize, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agent.ts")]
pub struct AgentStartResult {
    pub protocol: AgentProtocol,
    pub acp_version: u32,
}

impl AgentStartResult {
    /// Builds the only protocol result the current host accepts.
    pub fn acp_v1() -> Self {
        Self {
            protocol: AgentProtocol::Acp,
            acp_version: SUPPORTED_ACP_VERSION,
        }
    }
}

/// One model an agent offers before any session exists.
#[derive(Debug, Clone, Deserialize, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agent.ts")]
pub struct AgentModel {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub default: bool,
}

/// Result of the agent model discovery method.
#[derive(Debug, Clone, Deserialize, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agent.ts")]
pub struct AgentListModelsResult {
    pub models: Vec<AgentModel>,
}

/// Exports every agent control DTO into one TypeScript module.
pub(crate) fn export(config: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    AgentStartContext::export(config)?;
    AgentProtocol::export(config)?;
    AgentStartResult::export(config)?;
    AgentModel::export(config)?;
    AgentListModelsResult::export(config)?;
    Ok(())
}
