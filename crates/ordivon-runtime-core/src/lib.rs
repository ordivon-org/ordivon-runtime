//! Trusted-local Workspace execution and recovery core for Ordivon.
//!
//! This crate owns Workspace operations, transactional Job and Attempt state,
//! runner dispatch, process-tree ownership, bounded results and Artifacts,
//! reconciliation, Runtime inspection, and administrative repair semantics.

mod error;
mod file;
mod repo;
#[cfg(feature = "transactional-runtime")]
mod runtime;
mod search;
#[cfg(feature = "universal-executor")]
mod universal;

pub use error::{ExecError, ExecErrorCode};
pub use file::{
    read_many, read_text, ReadManyItem, ReadManyRequest, ReadManyResult, ReadTextRequest,
    ReadTextResult, MAX_BATCH_FILES, MAX_READ_BYTES, MAX_READ_LINES,
};
pub use repo::{repo_snapshot, RepoSnapshotRequest, RepoSnapshotResult};
pub use search::{
    search_text, SearchHit, SearchPatternMode, SearchSubmatch, SearchTextRequest, SearchTextResult,
    MAX_SEARCH_BYTES, MAX_SEARCH_GLOBS, MAX_SEARCH_RESULTS,
};

#[cfg(feature = "universal-executor")]
pub use universal::{
    create_git_workspace, create_git_workspace_compact, load_workspace_record, mutate_workspace,
    read_workspace_slice, read_workspace_slice_compact, read_workspace_text,
    read_workspace_text_compact, remove_git_workspace, run_task_runner, workspace_diff,
    workspace_diff_compact, write_workspace_text, CompactWorkspaceDiffResult,
    CompactWorkspaceOpenResult, CompactWorkspaceReadResult, CompactWorkspaceSliceResult,
    GitWorkspaceCreateRequest, UniversalExecError, UniversalExecErrorCode, UniversalExecutorConfig,
    WorkspaceCloseRequest, WorkspaceCloseResult, WorkspaceDiffRequest, WorkspaceDiffResult,
    WorkspaceMutateRequest, WorkspaceMutateResult, WorkspaceMutation, WorkspaceMutationMode,
    WorkspaceMutationResult, WorkspaceReadRequest, WorkspaceReadResult, WorkspaceReadSliceRequest,
    WorkspaceReadSliceResult, WorkspaceRecord, WorkspaceWriteRequest, WorkspaceWriteResult,
    MAX_UNIVERSAL_ARGS, MAX_UNIVERSAL_ARG_BYTES, MAX_UNIVERSAL_ENV_VALUE_BYTES,
    MAX_UNIVERSAL_ENV_VARS, MAX_UNIVERSAL_OUTPUT_BYTES, MAX_UNIVERSAL_RUNTIME_MS,
    MAX_WORKSPACE_IO_BYTES, MAX_WORKSPACE_MUTATIONS, UNIVERSAL_EXEC_SCHEMA_VERSION,
};

#[cfg(feature = "transactional-runtime")]
pub use runtime::{
    apply_runtime_repair, inspect_runtime, AdmissionOutcome, ArtifactDescriptor,
    ArtifactReadRequest, ArtifactReadResult, ArtifactRegistration, AttemptRecord, AttemptState,
    AttemptTerminationIntent, ConditionUpdate, CreatedAdmission, ExecutionBudget, JobDesiredState,
    JobProjection, JobResolution, ReconciliationFailure, ReconciliationReport, Registry,
    RegistryConfig, ReservationRecord, ReservationState, RunnerIdentity, Runtime,
    RuntimeArtifactRecord, RuntimeCapacity, RuntimeConfig, RuntimeDoctorAttemptState,
    RuntimeDoctorCapacityHolder, RuntimeDoctorCase, RuntimeDoctorConfig, RuntimeDoctorJobState,
    RuntimeDoctorProposal, RuntimeDoctorReport, RuntimeDoctorReservationState,
    RuntimeDoctorSummary, RuntimeError, RuntimeErrorCode, RuntimeExecutionPlan,
    RuntimeInvariantViolation, RuntimeJobListCursor, RuntimeJobListRequest, RuntimeJobListResult,
    RuntimeJobRecord, RuntimeJobSummary, RuntimeRepairAction, RuntimeRepairActionKind,
    RuntimeRepairConfig, RuntimeRepairReport, RuntimeRepairRequest, RuntimeResult, SubmitRequest,
    TaskCancelRequest, TaskObservation, TaskObserveRequest, TaskRunRequest, TerminalCommit,
    UniversalExecutionRequest, MAX_ARTIFACT_READ_BYTES, MAX_CPU_QUOTA_PERCENT,
    MAX_MEMORY_MAX_BYTES, MAX_RUNTIME_LIST_LIMIT, MAX_TASKS_MAX, MAX_TASK_TAIL_BYTES,
    MAX_TASK_WAIT_MS, MIN_MEMORY_MAX_BYTES, RUNTIME_DOCTOR_SCHEMA_VERSION,
    RUNTIME_MIGRATION_CHECKSUM, RUNTIME_ORPHAN_RECLAIM_MIGRATION_CHECKSUM,
    RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM, RUNTIME_REPAIR_SCHEMA_VERSION,
    RUNTIME_SCHEMA_VERSION, RUNTIME_TERMINAL_REPAIR_MIGRATION_CHECKSUM,
};
