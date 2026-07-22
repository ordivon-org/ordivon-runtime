mod engine;
mod error;
mod registry;
mod supervisor;
mod types;

pub use engine::{Runtime, RuntimeConfig};
pub use error::{RuntimeCapacity, RuntimeError, RuntimeErrorCode, RuntimeResult};
pub use registry::{
    Registry, RegistryConfig, RUNTIME_MIGRATION_CHECKSUM,
    RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM,
};
pub use types::{
    AdmissionOutcome, ArtifactDescriptor, ArtifactReadRequest, ArtifactReadResult,
    ArtifactRegistration, AttemptRecord, AttemptState, AttemptTerminationIntent, ConditionUpdate,
    CreatedAdmission, JobDesiredState, JobProjection, JobResolution, ReservationRecord,
    ReservationState, RunnerIdentity, RuntimeArtifactRecord, RuntimeExecutionPlan,
    RuntimeJobListCursor, RuntimeJobListRequest, RuntimeJobListResult, RuntimeJobRecord,
    SubmitRequest, TaskCancelRequest, TaskObservation, TaskObserveRequest, TaskRunRequest,
    TerminalCommit, UniversalExecutionRequest, MAX_RUNTIME_LIST_LIMIT, RUNTIME_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests;
