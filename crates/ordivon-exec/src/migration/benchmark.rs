use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    invalid, validate_identifier, validate_message, MigrationBackend, MigrationCapability,
    MigrationContractError, MigrationContractErrorCode, MigrationOperationClass,
    MIGRATION_CONTRACT_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationBenchmarkSample {
    pub schema_version: u32,
    pub sample_id: String,
    pub capability: MigrationCapability,
    pub backend: MigrationBackend,
    pub succeeded: bool,
    pub elapsed_ms: u64,
    pub tool_calls: u64,
    pub remote_round_trips: u64,
    pub context_bytes: u64,
    pub output_bytes: u64,
    pub recovered_after_disconnect: bool,
    pub fallback_count: u64,
}

impl MigrationBenchmarkSample {
    pub fn validate_shape(&self) -> Result<(), MigrationContractError> {
        if self.schema_version != MIGRATION_CONTRACT_SCHEMA_VERSION {
            return Err(invalid(
                "unsupported migration schema version",
                "schemaVersion",
            ));
        }
        validate_identifier(&self.sample_id, "sampleId")?;
        for (field, value) in [
            ("elapsedMs", self.elapsed_ms),
            ("toolCalls", self.tool_calls),
            ("remoteRoundTrips", self.remote_round_trips),
            ("contextBytes", self.context_bytes),
            ("outputBytes", self.output_bytes),
            ("fallbackCount", self.fallback_count),
        ] {
            if value > i64::MAX as u64 {
                return Err(MigrationContractError::new(
                    MigrationContractErrorCode::InvalidBenchmark,
                    format!("{field} exceeds the comparison range"),
                    field,
                ));
            }
        }
        if self.backend == MigrationBackend::Ordivon && self.fallback_count != 0 {
            return Err(MigrationContractError::new(
                MigrationContractErrorCode::InvalidBenchmark,
                "an Ordivon sample cannot record legacy fallback use",
                "fallbackCount",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationPerformanceDelta {
    pub capability: MigrationCapability,
    pub elapsed_ms_delta: i64,
    pub tool_calls_delta: i64,
    pub remote_round_trips_delta: i64,
    pub context_bytes_delta: i64,
    pub output_bytes_delta: i64,
    pub success_changed: bool,
    pub disconnect_recovery_improved: bool,
}

pub fn compare_migration_samples(
    legacy: &MigrationBenchmarkSample,
    ordivon: &MigrationBenchmarkSample,
) -> Result<MigrationPerformanceDelta, MigrationContractError> {
    legacy.validate_shape()?;
    ordivon.validate_shape()?;
    if legacy.backend != MigrationBackend::LegacyDesktopCommander
        || ordivon.backend != MigrationBackend::Ordivon
        || legacy.capability != ordivon.capability
    {
        return Err(MigrationContractError::new(
            MigrationContractErrorCode::IncomparableBenchmark,
            "samples must compare the same capability from legacy to Ordivon",
            "backend",
        ));
    }
    Ok(MigrationPerformanceDelta {
        capability: ordivon.capability.clone(),
        elapsed_ms_delta: delta(legacy.elapsed_ms, ordivon.elapsed_ms),
        tool_calls_delta: delta(legacy.tool_calls, ordivon.tool_calls),
        remote_round_trips_delta: delta(legacy.remote_round_trips, ordivon.remote_round_trips),
        context_bytes_delta: delta(legacy.context_bytes, ordivon.context_bytes),
        output_bytes_delta: delta(legacy.output_bytes, ordivon.output_bytes),
        success_changed: legacy.succeeded != ordivon.succeeded,
        disconnect_recovery_improved: !legacy.recovered_after_disconnect
            && ordivon.recovered_after_disconnect,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyFallbackRecord {
    pub schema_version: u32,
    pub record_id: String,
    pub capability: MigrationCapability,
    pub operation_class: MigrationOperationClass,
    pub legacy_tool: String,
    pub missing_or_degraded_capability: String,
    pub succeeded: bool,
    pub elapsed_ms: u64,
}

impl LegacyFallbackRecord {
    pub fn validate_shape(&self) -> Result<(), MigrationContractError> {
        if self.schema_version != MIGRATION_CONTRACT_SCHEMA_VERSION {
            return Err(invalid(
                "unsupported migration schema version",
                "schemaVersion",
            ));
        }
        validate_identifier(&self.record_id, "recordId")?;
        validate_identifier(&self.legacy_tool, "legacyTool")?;
        validate_message(
            &self.missing_or_degraded_capability,
            "missingOrDegradedCapability",
        )?;
        if !matches!(
            self.operation_class,
            MigrationOperationClass::ReadOnly | MigrationOperationClass::WorkspaceBoundMutation
        ) {
            return Err(invalid(
                "automatic legacy fallback records are limited to workspace-safe operations",
                "operationClass",
            ));
        }
        Ok(())
    }
}

fn delta(legacy: u64, ordivon: u64) -> i64 {
    ordivon as i64 - legacy as i64
}
