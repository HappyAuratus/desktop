use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Method that establishes a safe-to-mutate barrier for Agent-consumed Resources.
pub const EFFECT_COORDINATE_METHOD: &str = "effect/coordinate";
/// Method that reactivates an Agent after Resource mutation.
pub const EFFECT_REACTIVATE_METHOD: &str = "effect/reactivate";
/// Method that confirms an Agent consumed one exact Target projection.
pub const EFFECT_VERIFY_READY_METHOD: &str = "effect/verify_ready";
/// The one Skill materialization format currently accepted from Agent plugins.
pub const SKILL_DIRECTORY_V1: &str = "ora/skill-directory.v1";

/// Exact Target and Resource set sent around one mutation attempt.
#[derive(Debug, Clone, Deserialize, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "effect.ts")]
pub struct AgentEffectCoordinationContext {
    pub target_id: String,
    pub resource_ids: Vec<String>,
}

/// Exact immutable Target projection whose readiness the Agent must confirm.
#[derive(Debug, Clone, Deserialize, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "effect.ts")]
pub struct AgentEffectReadinessContext {
    pub target_id: String,
    #[ts(type = "number")]
    pub generation: u64,
    pub consumer_revision_id: String,
    pub projection_digest: String,
}

/// Exports every Effect adapter DTO into one TypeScript module.
pub(crate) fn export(config: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    AgentEffectCoordinationContext::export(config)?;
    AgentEffectReadinessContext::export(config)?;
    Ok(())
}
