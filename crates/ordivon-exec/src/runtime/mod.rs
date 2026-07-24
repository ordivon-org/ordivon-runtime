mod doctor;
mod engine;
mod error;
mod evidence;
mod registry;
mod supervisor;
mod types;

pub use doctor::{
    inspect_runtime, RuntimeDoctorAttemptState, RuntimeDoctorCase, RuntimeDoctorConfig,
    RuntimeDoctorJobState, RuntimeDoctorProposal, RuntimeDoctorReport,
    RuntimeDoctorReservationState, RUNTIME_DOCTOR_SCHEMA_VERSION,
};
pub use engine::{ReconciliationFailure, ReconciliationReport, Runtime, RuntimeConfig};
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
    RuntimeInvariantViolation, RuntimeJobListCursor, RuntimeJobListRequest, RuntimeJobListResult,
    RuntimeJobRecord, RuntimeJobSummary, SubmitRequest, TaskCancelRequest, TaskObservation,
    TaskObserveRequest, TaskRunRequest, TerminalCommit, UniversalExecutionRequest,
    MAX_ARTIFACT_READ_BYTES, MAX_RUNTIME_LIST_LIMIT, MAX_TASK_TAIL_BYTES, MAX_TASK_WAIT_MS,
    RUNTIME_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests;
