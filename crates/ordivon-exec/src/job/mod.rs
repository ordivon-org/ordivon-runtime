mod error;
mod io;
mod policy;
mod receipt;
mod state;
mod supervisor;
mod validation;

pub use error::{JobContractError, JobContractErrorCode};
pub use io::{
    JobListCursor, JobListRequest, JobListResult, JobOutputCursor, JobOutputEncoding,
    JobOutputStream, JobReadRequest, JobReadResult,
};
pub use policy::{
    CapabilityPolicy, EnvironmentRule, ExecutionPlan, ExecutionProfile, JobStartRequest,
};
pub use receipt::{OperationalEventOrigin, OperationalReceiptEvent, OperationalReceiptEventType};
pub use state::{
    JobInternalState, JobOutputMetadata, JobPublicState, JobRecord, JobStateTransition,
};
pub use supervisor::{
    classify_supervisor_recovery, classify_terminal_state, RecoveryEvidenceSource,
    RunnerResultObservation, SupervisorIdentity, SupervisorObservation,
    SupervisorRecoveryDisposition, SupervisorUnitState, TerminationIntent,
};

pub const JOB_CONTRACT_SCHEMA_VERSION: u32 = 1;
pub const MAX_JOB_ARGS: usize = 64;
pub const MAX_JOB_ARG_BYTES: usize = 4 * 1024;
pub const MAX_JOB_ENV_VARS: usize = 32;
pub const MAX_JOB_ENV_VALUE_BYTES: usize = 4 * 1024;
pub const MAX_JOB_OUTPUT_RETENTION_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_JOB_READ_BYTES: u64 = 1024 * 1024;
pub const MAX_JOB_LIST_LIMIT: u32 = 200;
pub(super) const MAX_IDENTIFIER_BYTES: usize = 256;

#[cfg(test)]
mod tests;
