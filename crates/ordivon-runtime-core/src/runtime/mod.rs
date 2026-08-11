mod doctor;
mod engine;
mod error;
mod evidence;
mod inspection;
mod patch;
mod registry;
mod repair;
mod supervisor;
mod systemd;
mod types;
mod windows;

pub use doctor::{
    inspect_runtime, RuntimeDoctorAttemptState, RuntimeDoctorCapacityHolder, RuntimeDoctorCase,
    RuntimeDoctorConfig, RuntimeDoctorJobState, RuntimeDoctorProposal, RuntimeDoctorReport,
    RuntimeDoctorReservationState, RuntimeDoctorSummary, RUNTIME_DOCTOR_SCHEMA_VERSION,
};
pub use engine::{ReconciliationFailure, ReconciliationReport, Runtime, RuntimeConfig};
pub use error::{RuntimeCapacity, RuntimeError, RuntimeErrorCode, RuntimeResult};
pub use inspection::{
    inspect_job, inspect_workspace, summarize_experience, RuntimeExperienceArtifactSummary,
    RuntimeExperienceCancellationSummary, RuntimeExperienceDispatchSummary,
    RuntimeExperienceDurationSummary, RuntimeExperienceJobSummary,
    RuntimeExperienceMechanicalLatencySummary, RuntimeExperienceRecoverySummary,
    RuntimeExperienceSummary, RuntimeInspectionArtifactSummary, RuntimeInspectionAttempt,
    RuntimeInspectionCondition, RuntimeInspectionConfig, RuntimeInspectionEpisodes,
    RuntimeInspectionEvent, RuntimeInspectionJob, RuntimeJobInspection, RuntimeWorkspaceInspection,
    RuntimeWorkspaceInspectionConfig, RuntimeWorkspaceInspectionJob,
    DEFAULT_INSPECTION_EVENT_LIMIT, DEFAULT_WORKSPACE_INSPECTION_JOB_LIMIT,
    MAX_INSPECTION_EVENT_LIMIT, MAX_WORKSPACE_INSPECTION_JOB_LIMIT,
    RUNTIME_INSPECTION_SCHEMA_VERSION,
};
pub use patch::{
    DurableWorkspacePatchRequest, DurableWorkspacePatchResult, WorkspacePatchOperationState,
    WorkspacePatchOperationStatus, WorkspacePatchStatusRequest,
};
pub use registry::{
    Registry, RegistryConfig, RUNTIME_MIGRATION_CHECKSUM,
    RUNTIME_ORPHAN_RECLAIM_MIGRATION_CHECKSUM, RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM,
    RUNTIME_TERMINAL_REPAIR_MIGRATION_CHECKSUM,
};
pub use repair::{
    apply_runtime_repair, RuntimeRepairAction, RuntimeRepairActionKind, RuntimeRepairConfig,
    RuntimeRepairReport, RuntimeRepairRequest, RUNTIME_REPAIR_SCHEMA_VERSION,
};
pub(crate) use types::{
    input_bound_proposal_request_identity_digest, input_bound_request_identity_digest,
    operation_request_identity_digest, operation_request_identity_digest_from_plan,
    proposal_request_identity_digest, validate_client_request_id, validate_logical_id,
    INPUT_BOUND_IDENTITY_PREFIX, INPUT_BOUND_PROPOSAL_IDENTITY_PREFIX, PROPOSAL_IDENTITY_PREFIX,
    REQUEST_IDENTITY_PREFIX, RUNTIME_RELEASE_IDENTITY_PREFIX,
};
pub use types::{
    runtime_release_effect_id, runtime_release_request_identity_digest, AdmissionOutcome,
    ArtifactDescriptor, ArtifactReadRequest, ArtifactReadResult, ArtifactRegistration,
    AttemptRecord, AttemptState, AttemptTerminationIntent, ConditionUpdate, CreatedAdmission,
    EffectiveExecutionLimits, EffectiveInputBinding, EffectiveStepTimeout, ExecutionBudget,
    ExecutionProfile, ExecutionProposal, ExecutionProviderContract, ExecutionProviderSnapshot,
    ExecutionStepProposal, ExecutionTarget, ForeignReference, HostDependencyBinding,
    InputAccessMode, InputAuthority, InputBindingRequest, JobDesiredState, JobProjection,
    JobResolution, ReservationRecord, ReservationState, RunnerIdentity, RuntimeArtifactRecord,
    RuntimeCapabilities, RuntimeDeliveryDisposition, RuntimeExecutionPlan, RuntimeExecutionStep,
    RuntimeExecutionTargetCapability, RuntimeInvariantViolation, RuntimeJobListCursor,
    RuntimeJobListRequest, RuntimeJobListResult, RuntimeJobRecord, RuntimeJobSummary,
    RuntimeReleaseAdmission, RuntimeReleaseContract, RuntimeReleaseDisposition,
    RuntimeReleaseEffectBinding, RuntimeReleaseGetRequest, RuntimeReleaseProjection,
    RuntimeReleaseRequest, RuntimeWorkspaceGetRequest, RuntimeWorkspaceIssue,
    RuntimeWorkspaceIssueStage, RuntimeWorkspaceListCursor, RuntimeWorkspaceListRequest,
    RuntimeWorkspaceListResult, RuntimeWorkspaceSummary, SubmitRequest, TaskCancelRequest,
    TaskObservation, TaskObserveRequest, TaskObserveWaitUntil, TaskRunProposal, TaskRunRequest,
    TerminalCommit, UniversalExecutionRequest, UniversalExecutionStep, WindowsAuthority,
    WindowsExecutionContext, WindowsTokenClass, CLIENT_REQUEST_ID_MAX_LENGTH,
    CLIENT_REQUEST_ID_MIN_LENGTH, CLIENT_REQUEST_ID_PATTERN, LOGICAL_ID_MAX_LENGTH,
    LOGICAL_ID_MIN_LENGTH, LOGICAL_ID_PATTERN, MAX_ARTIFACT_READ_BYTES, MAX_RUNTIME_LIST_LIMIT,
    MAX_TASK_TAIL_BYTES, MAX_TASK_WAIT_MS, RUNTIME_SCHEMA_VERSION,
};
pub use windows::WindowsExecutionConfig;

#[cfg(test)]
mod tests;
