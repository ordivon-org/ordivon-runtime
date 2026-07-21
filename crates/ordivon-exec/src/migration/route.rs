use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{invalid, MigrationContractError, MIGRATION_CONTRACT_SCHEMA_VERSION};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MigrationBackend {
    Ordivon,
    LegacyDesktopCommander,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MigrationCapability {
    WorkspaceRead,
    WorkspaceWrite,
    WorkspaceExec,
    TaskGet,
    TaskCancel,
    ArtifactRead,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MigrationOperationClass {
    ReadOnly,
    WorkspaceBoundMutation,
    TaskControl,
    HostOrExternalSideEffect,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BackendSupport {
    Available,
    Unavailable,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LegacyFallbackPolicy {
    Denied,
    WorkspaceOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MigrationRouteReason {
    OrdivonPrimary,
    LegacyFallback,
    OrdivonUnavailableFallbackDenied,
    LegacyUnavailable,
    UnsafeFallbackDenied,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationRouteRequest {
    pub schema_version: u32,
    pub capability: MigrationCapability,
    pub operation_class: MigrationOperationClass,
    pub ordivon_support: BackendSupport,
    pub legacy_support: BackendSupport,
    pub fallback_policy: LegacyFallbackPolicy,
    pub request_shadow_compare: bool,
}

impl MigrationRouteRequest {
    pub fn validate_shape(&self) -> Result<(), MigrationContractError> {
        if self.schema_version != MIGRATION_CONTRACT_SCHEMA_VERSION {
            return Err(invalid(
                "unsupported migration schema version",
                "schemaVersion",
            ));
        }
        if self.request_shadow_compare && self.operation_class != MigrationOperationClass::ReadOnly
        {
            return Err(invalid(
                "shadow comparison is only valid for read-only operations",
                "requestShadowCompare",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationRouteDecision {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_backend: Option<MigrationBackend>,
    pub reason: MigrationRouteReason,
    pub safe_to_execute: bool,
    pub fallback_used: bool,
    pub shadow_compare: bool,
}

pub fn decide_backend_route(
    request: &MigrationRouteRequest,
) -> Result<MigrationRouteDecision, MigrationContractError> {
    request.validate_shape()?;
    let ordivon_available = request.ordivon_support == BackendSupport::Available;
    let legacy_available = request.legacy_support == BackendSupport::Available;
    let shadow_compare = request.request_shadow_compare && ordivon_available && legacy_available;

    if ordivon_available {
        return Ok(MigrationRouteDecision {
            schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
            selected_backend: Some(MigrationBackend::Ordivon),
            reason: MigrationRouteReason::OrdivonPrimary,
            safe_to_execute: true,
            fallback_used: false,
            shadow_compare,
        });
    }

    let fallback_is_workspace_safe = matches!(
        request.operation_class,
        MigrationOperationClass::ReadOnly | MigrationOperationClass::WorkspaceBoundMutation
    );
    let fallback_allowed = request.fallback_policy == LegacyFallbackPolicy::WorkspaceOnly
        && fallback_is_workspace_safe;

    if fallback_allowed && legacy_available {
        return Ok(MigrationRouteDecision {
            schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
            selected_backend: Some(MigrationBackend::LegacyDesktopCommander),
            reason: MigrationRouteReason::LegacyFallback,
            safe_to_execute: true,
            fallback_used: true,
            shadow_compare: false,
        });
    }

    let reason = if !fallback_is_workspace_safe
        && request.fallback_policy == LegacyFallbackPolicy::WorkspaceOnly
    {
        MigrationRouteReason::UnsafeFallbackDenied
    } else if !legacy_available && request.fallback_policy == LegacyFallbackPolicy::WorkspaceOnly {
        MigrationRouteReason::LegacyUnavailable
    } else {
        MigrationRouteReason::OrdivonUnavailableFallbackDenied
    };
    Ok(MigrationRouteDecision {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        selected_backend: None,
        reason,
        safe_to_execute: false,
        fallback_used: false,
        shadow_compare: false,
    })
}
