//! Trusted-local Workspace execution and recovery core for Ordivon.
//!
//! This crate owns Workspace operations, transactional Job and Attempt state,
//! runner dispatch, process-tree ownership, bounded results and Artifacts,
//! reconciliation, Runtime inspection, and administrative repair semantics.

#[cfg(feature = "transactional-runtime")]
mod runtime;
#[cfg(feature = "universal-executor")]
mod universal;

#[cfg(feature = "universal-executor")]
pub use universal::{
    create_git_workspace, create_git_workspace_compact, list_workspace_records,
    load_workspace_record, mutate_workspace, patch_workspace, read_workspace_slice,
    read_workspace_slice_compact, read_workspace_text, read_workspace_text_compact,
    remove_git_workspace, run_task_runner, workspace_diff, workspace_diff_compact,
    write_workspace_text, CompactWorkspaceDiffResult, CompactWorkspaceOpenResult,
    CompactWorkspaceReadResult, CompactWorkspaceSliceResult, GitWorkspaceCreateRequest,
    UniversalExecError, UniversalExecErrorCode, UniversalExecutorConfig, WorkspaceCloseRequest,
    WorkspaceCloseResult, WorkspaceDiffRequest, WorkspaceDiffResult, WorkspaceFilePatch,
    WorkspaceMutateRequest, WorkspaceMutateResult, WorkspaceMutation, WorkspaceMutationMode,
    WorkspaceMutationResult, WorkspacePatchRequest, WorkspacePatchResult, WorkspacePatchedFile,
    WorkspaceReadRequest, WorkspaceReadResult, WorkspaceReadSliceRequest, WorkspaceReadSliceResult,
    WorkspaceRecord, WorkspaceRenamedPath, WorkspaceTextEdit, WorkspaceTextPosition,
    WorkspaceTextRange, WorkspaceWriteRequest, WorkspaceWriteResult, MAX_UNIVERSAL_ARGS,
    MAX_UNIVERSAL_ARG_BYTES, MAX_UNIVERSAL_ENV_VALUE_BYTES, MAX_UNIVERSAL_ENV_VARS,
    MAX_UNIVERSAL_OUTPUT_BYTES, MAX_UNIVERSAL_RUNTIME_MS, MAX_WORKSPACE_IO_BYTES,
    MAX_WORKSPACE_MUTATIONS, MAX_WORKSPACE_PATCH_EDITS_PER_FILE, MAX_WORKSPACE_PATCH_FILES,
    UNIVERSAL_EXEC_SCHEMA_VERSION,
};

#[cfg(feature = "transactional-runtime")]
pub use runtime::{
    apply_runtime_repair, inspect_job, inspect_runtime, summarize_experience, AdmissionOutcome,
    ArtifactDescriptor, ArtifactReadRequest, ArtifactReadResult, ArtifactRegistration,
    AttemptRecord, AttemptState, AttemptTerminationIntent, ConditionUpdate, CreatedAdmission,
    ExecutionBudget, ExecutionProfile, ForeignReference, JobDesiredState, JobProjection,
    JobResolution, ReconciliationFailure, ReconciliationReport, Registry, RegistryConfig,
    ReservationRecord, ReservationState, RunnerIdentity, Runtime, RuntimeArtifactRecord,
    RuntimeCapacity, RuntimeConfig, RuntimeDoctorAttemptState, RuntimeDoctorCapacityHolder,
    RuntimeDoctorCase, RuntimeDoctorConfig, RuntimeDoctorJobState, RuntimeDoctorProposal,
    RuntimeDoctorReport, RuntimeDoctorReservationState, RuntimeDoctorSummary, RuntimeError,
    RuntimeErrorCode, RuntimeExecutionPlan, RuntimeExecutionStep, RuntimeExperienceArtifactSummary,
    RuntimeExperienceCancellationSummary, RuntimeExperienceDispatchSummary,
    RuntimeExperienceDurationSummary, RuntimeExperienceJobSummary,
    RuntimeExperienceMechanicalLatencySummary, RuntimeExperienceRecoverySummary,
    RuntimeExperienceSummary, RuntimeInspectionArtifactSummary, RuntimeInspectionAttempt,
    RuntimeInspectionCondition, RuntimeInspectionConfig, RuntimeInspectionEpisodes,
    RuntimeInspectionEvent, RuntimeInspectionJob, RuntimeInvariantViolation, RuntimeJobInspection,
    RuntimeJobListCursor, RuntimeJobListRequest, RuntimeJobListResult, RuntimeJobRecord,
    RuntimeJobSummary, RuntimeRepairAction, RuntimeRepairActionKind, RuntimeRepairConfig,
    RuntimeRepairReport, RuntimeRepairRequest, RuntimeResult, RuntimeWorkspaceGetRequest,
    RuntimeWorkspaceIssue, RuntimeWorkspaceListRequest, RuntimeWorkspaceListResult,
    RuntimeWorkspaceSummary, SubmitRequest, TaskCancelRequest, TaskObservation, TaskObserveRequest,
    TaskObserveWaitUntil, TaskRunRequest, TerminalCommit, UniversalExecutionRequest,
    UniversalExecutionStep, DEFAULT_INSPECTION_EVENT_LIMIT, MAX_ARTIFACT_READ_BYTES,
    MAX_CPU_QUOTA_PERCENT, MAX_FOREIGN_REFERENCES, MAX_INSPECTION_EVENT_LIMIT,
    MAX_MEMORY_MAX_BYTES, MAX_RUNTIME_LIST_LIMIT, MAX_TASKS_MAX, MAX_TASK_TAIL_BYTES,
    MAX_TASK_WAIT_MS, MIN_MEMORY_MAX_BYTES, RUNTIME_DOCTOR_SCHEMA_VERSION,
    RUNTIME_INSPECTION_SCHEMA_VERSION, RUNTIME_MIGRATION_CHECKSUM,
    RUNTIME_ORPHAN_RECLAIM_MIGRATION_CHECKSUM, RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM,
    RUNTIME_REPAIR_SCHEMA_VERSION, RUNTIME_SCHEMA_VERSION,
    RUNTIME_TERMINAL_REPAIR_MIGRATION_CHECKSUM,
};
