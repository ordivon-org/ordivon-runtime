mod engine;
mod error;
mod registry;
mod types;

pub use engine::{Runtime, RuntimeConfig};
pub use error::{RuntimeError, RuntimeErrorCode, RuntimeResult};
pub use registry::{Registry, RegistryConfig, RUNTIME_MIGRATION_CHECKSUM};
pub use types::{
    AdmissionOutcome, ArtifactReadRequest, ArtifactReadResult, ArtifactRegistration, AttemptRecord,
    AttemptState, AttemptTerminationIntent, ConditionUpdate, CreatedAdmission, JobDesiredState,
    JobProjection, JobResolution, PlanKind, ReservationRecord, ReservationState, RunnerIdentity,
    RuntimeArtifactRecord, RuntimeExecutionPlan, RuntimeJobListCursor, RuntimeJobListRequest,
    RuntimeJobListResult, RuntimeJobRecord, SubmitRequest, TaskCancelRequest, TaskObservation,
    TaskObserveRequest, TaskRunRequest, TerminalCommit, UniversalExecutionRequest,
    MAX_RUNTIME_LIST_LIMIT, RUNTIME_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests;
