mod error;
mod registry;
mod runtime;
mod types;

pub use error::{M6Error, M6ErrorCode, M6Result};
#[cfg(feature = "runtime-hardening-m7")]
pub use registry::M7_MIGRATION_CHECKSUM;
pub use registry::{M6Registry, M6RegistryConfig, M6_MIGRATION_CHECKSUM};
pub use runtime::{M6Runtime, M6RuntimeConfig};
pub use types::{
    AdmissionOutcomeM6, ArtifactRecordM6, ArtifactRegistrationM6, AttemptRecordM6, AttemptState,
    ConditionUpdateM6, CreatedAdmissionM6, JobDesiredState, JobListCursorM6, JobListRequestM6,
    JobListResultM6, JobProjectionM6, JobRecordM6, JobResolution, M6ArtifactReadRequest,
    M6ArtifactReadResult, M6ExecutionPlan, M6SubmitRequest, M6TaskCancelRequest, M6TaskObservation,
    M6TaskObserveRequest, M6TaskRunRequest, M6TerminationIntent, M6UniversalExecutionRequest,
    PlanKind, ReservationRecordM6, ReservationState, RunnerIdentityM6, TerminalCommitM6,
    M6_SCHEMA_VERSION, MAX_M6_LIST_LIMIT,
};

#[cfg(test)]
mod tests;
