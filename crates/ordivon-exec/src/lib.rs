mod error;
mod file;
mod job;
#[cfg(feature = "transactional-registry-m6")]
mod m6;
mod migration;
mod repo;
mod search;
#[cfg(feature = "universal-executor-m1")]
mod universal;

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
pub use migration::{
    compare_migration_samples, decide_backend_route, ArtifactKind, ArtifactReference,
    BackendSupport, LegacyFallbackPolicy, LegacyFallbackRecord, MigrationBackend,
    MigrationBenchmarkSample, MigrationCapability, MigrationContractError,
    MigrationContractErrorCode, MigrationOperationClass, MigrationPerformanceDelta,
    MigrationRouteDecision, MigrationRouteReason, MigrationRouteRequest, MigrationTaskHandle,
    MigrationTaskStatus, TaskInputRequest, MAX_INPUT_OPTIONS, MAX_POLL_AFTER_MS,
    MAX_STATUS_MESSAGE_BYTES, MAX_TASK_ARTIFACTS, MIGRATION_CONTRACT_SCHEMA_VERSION,
};
pub use repo::{repo_snapshot, RepoSnapshotRequest, RepoSnapshotResult};
pub use search::{
    search_text, SearchHit, SearchPatternMode, SearchSubmatch, SearchTextRequest, SearchTextResult,
    MAX_SEARCH_BYTES, MAX_SEARCH_GLOBS, MAX_SEARCH_RESULTS,
};

#[cfg(feature = "universal-executor-m1")]
pub use universal::{
    await_universal_task_compact, cancel_universal_task, create_git_workspace,
    create_git_workspace_compact, get_universal_task, load_workspace_record, mutate_workspace,
    read_task_artifact, read_workspace_slice, read_workspace_slice_compact, read_workspace_text,
    read_workspace_text_compact, remove_git_workspace, run_task_runner, run_universal_task_compact,
    snapshot_universal_task, start_universal_task, workspace_diff, workspace_diff_compact,
    write_workspace_text, ArtifactReadRequest, ArtifactReadResult, CompactTaskObservation,
    CompactWorkspaceDiffResult, CompactWorkspaceOpenResult, CompactWorkspaceReadResult,
    CompactWorkspaceSliceResult, DurableTaskSnapshot, GitWorkspaceCreateRequest, TaskAwaitRequest,
    TaskCancelRequest, TaskGetRequest, TaskRunRequest, UniversalExecError, UniversalExecErrorCode,
    UniversalExecRequest, UniversalExecutorConfig, WorkspaceDiffRequest, WorkspaceDiffResult,
    WorkspaceMutateRequest, WorkspaceMutateResult, WorkspaceMutation, WorkspaceMutationMode,
    WorkspaceMutationResult, WorkspaceReadRequest, WorkspaceReadResult, WorkspaceReadSliceRequest,
    WorkspaceReadSliceResult, WorkspaceRecord, WorkspaceWriteRequest, WorkspaceWriteResult,
    MAX_ARTIFACT_READ_BYTES, MAX_COMPACT_TAIL_BYTES, MAX_TASK_WAIT_MS, MAX_UNIVERSAL_ARGS,
    MAX_UNIVERSAL_ARG_BYTES, MAX_UNIVERSAL_ENV_VALUE_BYTES, MAX_UNIVERSAL_ENV_VARS,
    MAX_UNIVERSAL_OUTPUT_BYTES, MAX_UNIVERSAL_RUNTIME_MS, MAX_WORKSPACE_IO_BYTES,
    MAX_WORKSPACE_MUTATIONS, UNIVERSAL_EXEC_SCHEMA_VERSION,
};

#[cfg(feature = "transactional-registry-m6")]
pub use m6::{
    AdmissionOutcomeM6, ArtifactRecordM6, ArtifactRegistrationM6, AttemptRecordM6, AttemptState,
    ConditionUpdateM6, CreatedAdmissionM6, JobDesiredState, JobListCursorM6, JobListRequestM6,
    JobListResultM6, JobProjectionM6, JobRecordM6, JobResolution, M6ArtifactReadRequest,
    M6ArtifactReadResult, M6Error, M6ErrorCode, M6ExecutionPlan, M6Registry, M6RegistryConfig,
    M6Result, M6Runtime, M6RuntimeConfig, M6SubmitRequest, M6TaskCancelRequest, M6TaskObservation,
    M6TaskObserveRequest, M6TaskRunRequest, M6TerminationIntent, M6UniversalExecutionRequest,
    PlanKind, ReservationRecordM6, ReservationState, RunnerIdentityM6, TerminalCommitM6,
    M6_MIGRATION_CHECKSUM, M6_SCHEMA_VERSION, MAX_M6_LIST_LIMIT,
};
