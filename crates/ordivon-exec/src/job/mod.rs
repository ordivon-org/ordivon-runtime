mod error;
mod io;
mod policy;
mod receipt;
mod resolver;
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
pub use resolver::{
    evaluate_job_start, load_capability_policy_bytes, load_capability_policy_file,
    CapabilityEvaluationContext, ConcurrencySnapshot, ResolvedCapabilityPolicy,
    MAX_ALLOWED_ARGUMENT_VECTORS, MAX_CAPABILITY_POLICY_BYTES, MAX_CAPABILITY_PROFILES,
    MAX_CAPABILITY_ROOTS, MAX_EXECUTION_CONCURRENCY, MAX_EXECUTION_RUNTIME_MS,
};
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
