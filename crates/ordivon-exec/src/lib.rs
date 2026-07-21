mod error;
mod file;
mod job;
mod repo;
mod search;

pub use error::{ExecError, ExecErrorCode};
pub use file::{
    read_many, read_text, ReadManyItem, ReadManyRequest, ReadManyResult, ReadTextRequest,
    ReadTextResult, MAX_BATCH_FILES, MAX_READ_BYTES, MAX_READ_LINES,
};
pub use job::{
    classify_supervisor_recovery, classify_terminal_state, evaluate_job_start,
    load_capability_policy_bytes, load_capability_policy_file, CapabilityEvaluationContext,
    CapabilityPolicy, ConcurrencySnapshot, EnvironmentRule, ExecutionPlan, ExecutionProfile,
    JobContractError, JobContractErrorCode, JobInternalState, JobListCursor, JobListRequest,
    JobListResult, JobOutputCursor, JobOutputEncoding, JobOutputMetadata, JobOutputStream,
    JobPublicState, JobReadRequest, JobReadResult, JobRecord, JobStartRequest, JobStateTransition,
    OperationalEventOrigin, OperationalReceiptEvent, OperationalReceiptEventType,
    RecoveryEvidenceSource, ResolvedCapabilityPolicy, RunnerResultObservation, SupervisorIdentity,
    SupervisorObservation, SupervisorRecoveryDisposition, SupervisorUnitState, TerminationIntent,
    JOB_CONTRACT_SCHEMA_VERSION, MAX_ALLOWED_ARGUMENT_VECTORS, MAX_CAPABILITY_POLICY_BYTES,
    MAX_CAPABILITY_PROFILES, MAX_CAPABILITY_ROOTS, MAX_EXECUTION_CONCURRENCY,
    MAX_EXECUTION_RUNTIME_MS, MAX_JOB_ARGS, MAX_JOB_ARG_BYTES, MAX_JOB_ENV_VALUE_BYTES,
    MAX_JOB_ENV_VARS, MAX_JOB_LIST_LIMIT, MAX_JOB_OUTPUT_RETENTION_BYTES, MAX_JOB_READ_BYTES,
};
pub use repo::{repo_snapshot, RepoSnapshotRequest, RepoSnapshotResult};
pub use search::{
    search_text, SearchHit, SearchPatternMode, SearchSubmatch, SearchTextRequest, SearchTextResult,
    MAX_SEARCH_BYTES, MAX_SEARCH_GLOBS, MAX_SEARCH_RESULTS,
};
