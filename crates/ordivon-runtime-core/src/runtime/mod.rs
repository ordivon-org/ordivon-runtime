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

pub use doctor::{
    inspect_runtime, RuntimeDoctorAttemptState, RuntimeDoctorCapacityHolder, RuntimeDoctorCase,
    RuntimeDoctorConfig, RuntimeDoctorJobState, RuntimeDoctorProposal, RuntimeDoctorReport,
    RuntimeDoctorReservationState, RuntimeDoctorSummary, RUNTIME_DOCTOR_SCHEMA_VERSION,
};
pub use engine::{ReconciliationFailure, ReconciliationReport, Runtime, RuntimeConfig};
pub use error::{RuntimeCapacity, RuntimeError, RuntimeErrorCode, RuntimeResult};
pub use inspection::{
    inspect_job, summarize_experience, RuntimeExperienceArtifactSummary,
    RuntimeExperienceCancellationSummary, RuntimeExperienceDispatchSummary,
    RuntimeExperienceDurationSummary, RuntimeExperienceJobSummary,
    RuntimeExperienceMechanicalLatencySummary, RuntimeExperienceRecoverySummary,
    RuntimeExperienceSummary, RuntimeInspectionArtifactSummary, RuntimeInspectionAttempt,
    RuntimeInspectionCondition, RuntimeInspectionConfig, RuntimeInspectionEpisodes,
    RuntimeInspectionEvent, RuntimeInspectionJob, RuntimeJobInspection,
    DEFAULT_INSPECTION_EVENT_LIMIT, MAX_INSPECTION_EVENT_LIMIT, RUNTIME_INSPECTION_SCHEMA_VERSION,
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
    operation_request_identity_digest, operation_request_identity_digest_from_plan,
    REQUEST_IDENTITY_PREFIX,
};
pub use types::{
    AdmissionOutcome, ArtifactDescriptor, ArtifactReadRequest, ArtifactReadResult,
    ArtifactRegistration, AttemptRecord, AttemptState, AttemptTerminationIntent, ConditionUpdate,
    CreatedAdmission, ExecutionBudget, ExecutionProfile, ForeignReference, JobDesiredState,
    JobProjection, JobResolution, ReservationRecord, ReservationState, RunnerIdentity,
    RuntimeArtifactRecord, RuntimeDeliveryDisposition, RuntimeExecutionPlan, RuntimeExecutionStep,
    RuntimeInvariantViolation, RuntimeJobListCursor, RuntimeJobListRequest, RuntimeJobListResult,
    RuntimeJobRecord, RuntimeJobSummary, RuntimeWorkspaceGetRequest, RuntimeWorkspaceIssue,
    RuntimeWorkspaceIssueStage, RuntimeWorkspaceListRequest, RuntimeWorkspaceListResult,
    RuntimeWorkspaceSummary, SubmitRequest, TaskCancelRequest, TaskObservation, TaskObserveRequest,
    TaskObserveWaitUntil, TaskRunRequest, TerminalCommit, UniversalExecutionRequest,
    UniversalExecutionStep, MAX_ARTIFACT_READ_BYTES, MAX_CPU_QUOTA_PERCENT, MAX_FOREIGN_REFERENCES,
    MAX_MEMORY_MAX_BYTES, MAX_RUNTIME_LIST_LIMIT, MAX_TASKS_MAX, MAX_TASK_TAIL_BYTES,
    MAX_TASK_WAIT_MS, MIN_MEMORY_MAX_BYTES, RUNTIME_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests;
