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
    create_git_workspace, create_git_workspace_compact, inspect_workspace_patch_plan,
    list_workspace_records, load_workspace_record, mutate_workspace, patch_workspace,
    plan_workspace_patch, read_workspace_content, read_workspace_slice,
    read_workspace_slice_compact, read_workspace_text, read_workspace_text_compact,
    remove_git_workspace, result_from_workspace_patch_plan, run_task_runner,
    workspace_changes_page, workspace_diff, workspace_diff_compact, workspace_head_revision,
    workspace_is_dirty, write_workspace_text, CompactWorkspaceDiffResult,
    CompactWorkspaceOpenResult, CompactWorkspaceReadResult, CompactWorkspaceSliceResult,
    GitWorkspaceCreateRequest, UniversalExecError, UniversalExecErrorCode, UniversalExecutorConfig,
    WorkspaceChangeCursor, WorkspaceChangeEntry, WorkspaceChangeKind, WorkspaceChangePageRequest,
    WorkspaceChangePageResult, WorkspaceCloseRequest, WorkspaceCloseResult,
    WorkspaceClosureDisposition, WorkspaceContentMetadata, WorkspaceContentReadResult,
    WorkspaceContentRequest, WorkspaceDiffRequest, WorkspaceDiffResult, WorkspaceFilePatch,
    WorkspaceMutateRequest, WorkspaceMutateResult, WorkspaceMutation, WorkspaceMutationMode,
    WorkspaceMutationResult, WorkspacePatchPlan, WorkspacePatchPlanFile, WorkspacePatchPlanState,
    WorkspacePatchRequest, WorkspacePatchResult, WorkspacePatchedFile, WorkspaceReadRequest,
    WorkspaceReadResult, WorkspaceReadSliceRequest, WorkspaceReadSliceResult, WorkspaceRecord,
    WorkspaceRenamedPath, WorkspaceTextEdit, WorkspaceTextPosition, WorkspaceTextRange,
    WorkspaceWriteRequest, WorkspaceWriteResult, MAX_WORKSPACE_CHANGE_PAGE_ENTRIES,
    MAX_WORKSPACE_IO_BYTES, UNIVERSAL_EXEC_SCHEMA_VERSION,
};

#[cfg(feature = "transactional-runtime")]
pub use runtime::{
    apply_runtime_repair, inspect_job, inspect_runtime, inspect_workspace, summarize_experience,
    AdmissionOutcome, ArtifactDescriptor, ArtifactReadRequest, ArtifactReadResult,
    ArtifactRegistration, AttemptRecord, AttemptState, AttemptTerminationIntent, ConditionUpdate,
    CreatedAdmission, DurableWorkspacePatchRequest, DurableWorkspacePatchResult,
    EffectiveExecutionLimits, EffectiveInputBinding, EffectiveStepTimeout, ExecutionBudget,
    ExecutionProfile, ExecutionProposal, ExecutionStepProposal, ExecutionTarget, ForeignReference,
    InputAccessMode, InputAuthority, InputBindingRequest, JobDesiredState, JobProjection,
    JobResolution, ReconciliationFailure, ReconciliationReport, Registry, RegistryConfig,
    ReservationRecord, ReservationState, RunnerIdentity, Runtime, RuntimeArtifactRecord,
    RuntimeCapacity, RuntimeConfig, RuntimeDeliveryDisposition, RuntimeDoctorAttemptState,
    RuntimeDoctorCapacityHolder, RuntimeDoctorCase, RuntimeDoctorConfig, RuntimeDoctorJobState,
    RuntimeDoctorProposal, RuntimeDoctorReport, RuntimeDoctorReservationState,
    RuntimeDoctorSummary, RuntimeError, RuntimeErrorCode, RuntimeExecutionPlan,
    RuntimeExecutionStep, RuntimeExperienceArtifactSummary, RuntimeExperienceCancellationSummary,
    RuntimeExperienceDispatchSummary, RuntimeExperienceDurationSummary,
    RuntimeExperienceJobSummary, RuntimeExperienceMechanicalLatencySummary,
    RuntimeExperienceRecoverySummary, RuntimeExperienceSummary, RuntimeInspectionArtifactSummary,
    RuntimeInspectionAttempt, RuntimeInspectionCondition, RuntimeInspectionConfig,
    RuntimeInspectionEpisodes, RuntimeInspectionEvent, RuntimeInspectionJob,
    RuntimeInvariantViolation, RuntimeJobInspection, RuntimeJobListCursor, RuntimeJobListRequest,
    RuntimeJobListResult, RuntimeJobRecord, RuntimeJobSummary, RuntimeRepairAction,
    RuntimeRepairActionKind, RuntimeRepairConfig, RuntimeRepairReport, RuntimeRepairRequest,
    RuntimeResult, RuntimeWorkspaceGetRequest, RuntimeWorkspaceInspection,
    RuntimeWorkspaceInspectionConfig, RuntimeWorkspaceInspectionJob, RuntimeWorkspaceIssue,
    RuntimeWorkspaceIssueStage, RuntimeWorkspaceListCursor, RuntimeWorkspaceListRequest,
    RuntimeWorkspaceListResult, RuntimeWorkspaceSummary, SubmitRequest, TaskCancelRequest,
    TaskObservation, TaskObserveRequest, TaskObserveWaitUntil, TaskRunProposal, TaskRunRequest,
    TerminalCommit, UniversalExecutionRequest, UniversalExecutionStep, WindowsExecutionConfig,
    WorkspacePatchOperationState, WorkspacePatchOperationStatus, WorkspacePatchStatusRequest,
    DEFAULT_INSPECTION_EVENT_LIMIT, DEFAULT_WORKSPACE_INSPECTION_JOB_LIMIT,
    MAX_ARTIFACT_READ_BYTES, MAX_INSPECTION_EVENT_LIMIT, MAX_RUNTIME_LIST_LIMIT,
    MAX_TASK_TAIL_BYTES, MAX_TASK_WAIT_MS, MAX_WORKSPACE_INSPECTION_JOB_LIMIT,
    RUNTIME_DOCTOR_SCHEMA_VERSION, RUNTIME_INSPECTION_SCHEMA_VERSION, RUNTIME_MIGRATION_CHECKSUM,
    RUNTIME_ORPHAN_RECLAIM_MIGRATION_CHECKSUM, RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM,
    RUNTIME_REPAIR_SCHEMA_VERSION, RUNTIME_SCHEMA_VERSION,
    RUNTIME_TERMINAL_REPAIR_MIGRATION_CHECKSUM,
};
