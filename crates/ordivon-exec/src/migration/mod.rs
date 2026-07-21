mod benchmark;
mod error;
mod route;
mod task;
mod validation;

pub use benchmark::{
    compare_migration_samples, LegacyFallbackRecord, MigrationBenchmarkSample,
    MigrationPerformanceDelta,
};
pub use error::{MigrationContractError, MigrationContractErrorCode};
pub use route::{
    decide_backend_route, BackendSupport, LegacyFallbackPolicy, MigrationBackend,
    MigrationCapability, MigrationOperationClass, MigrationRouteDecision, MigrationRouteReason,
    MigrationRouteRequest,
};
pub use task::{
    ArtifactKind, ArtifactReference, MigrationTaskHandle, MigrationTaskStatus, TaskInputRequest,
};
pub(crate) use validation::{invalid, validate_digest, validate_identifier, validate_message};

pub const MIGRATION_CONTRACT_SCHEMA_VERSION: u32 = 1;
pub const MAX_TASK_ARTIFACTS: usize = 64;
pub const MAX_INPUT_OPTIONS: usize = 8;
pub const MAX_STATUS_MESSAGE_BYTES: usize = 1024;
pub const MAX_POLL_AFTER_MS: u64 = 60_000;

#[cfg(test)]
mod tests;
