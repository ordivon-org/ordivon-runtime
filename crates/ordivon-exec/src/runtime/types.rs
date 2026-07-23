use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{RuntimeError, RuntimeResult};

pub const RUNTIME_SCHEMA_VERSION: u32 = 1;
pub const MAX_RUNTIME_LIST_LIMIT: u32 = 100;
pub const MAX_TASK_WAIT_MS: u64 = 30_000;
pub const MAX_TASK_TAIL_BYTES: u64 = 64 * 1024;
pub const MAX_ARTIFACT_READ_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobDesiredState {
    Run,
    Cancelled,
}

impl JobDesiredState {
    pub(crate) fn as_db(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Cancelled => "cancelled",
        }
    }

    pub(crate) fn parse(value: &str) -> RuntimeResult<Self> {
        match value {
            "run" => Ok(Self::Run),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(RuntimeError::invalid(
                "unknown desired state",
                "desiredState",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobResolution {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Lost,
    Orphaned,
}

impl JobResolution {
    pub(crate) fn as_db(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::Lost => "lost",
            Self::Orphaned => "orphaned",
        }
    }

    pub(crate) fn parse(value: &str) -> RuntimeResult<Self> {
        match value {
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "timed_out" => Ok(Self::TimedOut),
            "cancelled" => Ok(Self::Cancelled),
            "lost" => Ok(Self::Lost),
            "orphaned" => Ok(Self::Orphaned),
            _ => Err(RuntimeError::invalid(
                "unknown job resolution",
                "resolution",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Accepted,
    Starting,
    Running,
    Stopping,
    Recovering,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Lost,
    Orphaned,
}

impl AttemptState {
    pub(crate) fn as_db(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Recovering => "recovering",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::Lost => "lost",
            Self::Orphaned => "orphaned",
        }
    }

    pub(crate) fn parse(value: &str) -> RuntimeResult<Self> {
        match value {
            "accepted" => Ok(Self::Accepted),
            "starting" => Ok(Self::Starting),
            "running" => Ok(Self::Running),
            "stopping" => Ok(Self::Stopping),
            "recovering" => Ok(Self::Recovering),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "timed_out" => Ok(Self::TimedOut),
            "cancelled" => Ok(Self::Cancelled),
            "lost" => Ok(Self::Lost),
            "orphaned" => Ok(Self::Orphaned),
            _ => Err(RuntimeError::invalid("unknown attempt state", "state")),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::TimedOut
                | Self::Cancelled
                | Self::Lost
                | Self::Orphaned
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Accepted => matches!(
                next,
                Self::Starting | Self::Cancelled | Self::Failed | Self::Lost | Self::Orphaned
            ),
            Self::Starting => matches!(
                next,
                Self::Running
                    | Self::Recovering
                    | Self::Succeeded
                    | Self::Failed
                    | Self::TimedOut
                    | Self::Cancelled
                    | Self::Lost
                    | Self::Orphaned
            ),
            Self::Running => matches!(
                next,
                Self::Stopping
                    | Self::Recovering
                    | Self::Succeeded
                    | Self::Failed
                    | Self::TimedOut
                    | Self::Cancelled
                    | Self::Lost
                    | Self::Orphaned
            ),
            Self::Stopping => matches!(
                next,
                Self::Recovering
                    | Self::Cancelled
                    | Self::Failed
                    | Self::TimedOut
                    | Self::Lost
                    | Self::Orphaned
            ),
            Self::Recovering => matches!(
                next,
                Self::Starting
                    | Self::Running
                    | Self::Stopping
                    | Self::Succeeded
                    | Self::Failed
                    | Self::TimedOut
                    | Self::Cancelled
                    | Self::Lost
                    | Self::Orphaned
            ),
            Self::Succeeded
            | Self::Failed
            | Self::TimedOut
            | Self::Cancelled
            | Self::Lost
            | Self::Orphaned => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptTerminationIntent {
    Natural,
    StopRequested,
    DeadlineExceeded,
}

impl AttemptTerminationIntent {
    pub(crate) fn as_db(self) -> &'static str {
        match self {
            Self::Natural => "natural",
            Self::StopRequested => "stop_requested",
            Self::DeadlineExceeded => "deadline_exceeded",
        }
    }

    pub(crate) fn parse(value: &str) -> RuntimeResult<Self> {
        match value {
            "natural" => Ok(Self::Natural),
            "stop_requested" => Ok(Self::StopRequested),
            "deadline_exceeded" => Ok(Self::DeadlineExceeded),
            _ => Err(RuntimeError::invalid(
                "unknown termination intent",
                "terminationIntent",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationState {
    Active,
    HeldOrphaned,
    Released,
}

impl ReservationState {
    pub(crate) fn as_db(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::HeldOrphaned => "held_orphaned",
            Self::Released => "released",
        }
    }

    pub(crate) fn parse(value: &str) -> RuntimeResult<Self> {
        match value {
            "active" => Ok(Self::Active),
            "held_orphaned" => Ok(Self::HeldOrphaned),
            "released" => Ok(Self::Released),
            _ => Err(RuntimeError::invalid(
                "unknown reservation state",
                "reservationState",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeExecutionPlan {
    pub schema_version: u32,
    pub workspace_id: String,
    pub workspace_path: String,
    pub source_revision: String,
    pub executable: String,
    pub executable_digest: String,
    #[serde(default)]
    #[schemars(length(max = 128))]
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[schemars(range(min = 1, max = crate::MAX_UNIVERSAL_RUNTIME_MS))]
    pub timeout_ms: u64,
    #[schemars(range(min = 1, max = crate::MAX_UNIVERSAL_OUTPUT_BYTES))]
    pub stdout_limit_bytes: u64,
    #[schemars(range(min = 1, max = crate::MAX_UNIVERSAL_OUTPUT_BYTES))]
    pub stderr_limit_bytes: u64,
    pub principal: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitRequest {
    pub schema_version: u32,
    pub client_request_id: String,
    pub plan: RuntimeExecutionPlan,
    pub global_limit: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeJobRecord {
    pub job_id: String,
    pub principal: String,
    pub client_request_id: String,
    pub request_digest: String,
    pub operation_digest: String,
    pub workspace_id: String,
    pub workspace_snapshot_json: String,
    pub execution_plan_json: String,
    pub execution_plan_digest: String,
    pub created_at_ms: u64,
    pub desired_state: JobDesiredState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<JobResolution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_attempt_id: Option<String>,
    pub row_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptRecord {
    pub attempt_id: String,
    pub job_id: String,
    pub attempt_number: u32,
    pub state: AttemptState,
    pub termination_intent: AttemptTerminationIntent,
    pub launch_token_digest: String,
    pub bundle_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_id: Option<String>,
    pub unit_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_start_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_start_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infrastructure_error_digest: Option<String>,
    pub created_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    pub row_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReservationRecord {
    pub reservation_id: String,
    pub attempt_id: String,
    pub global_limit: u32,
    pub state: ReservationState,
    pub acquired_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub released_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatedAdmission {
    pub job: RuntimeJobRecord,
    pub attempt: AttemptRecord,
    pub reservation: ReservationRecord,
    pub launch_token: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum AdmissionOutcome {
    Created(Box<CreatedAdmission>),
    Existing { job: Box<RuntimeJobRecord> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactRegistration {
    pub artifact_id: String,
    pub kind: String,
    pub relative_path: String,
    pub digest: String,
    pub media_type: String,
    pub byte_length: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactDescriptor {
    pub artifact_id: String,
    pub kind: String,
    pub digest: String,
    pub retained_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dropped_bytes: Option<u64>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeArtifactRecord {
    pub artifact_id: String,
    pub job_id: String,
    pub attempt_id: String,
    pub kind: String,
    pub relative_path: String,
    pub digest: String,
    pub media_type: String,
    pub byte_length: u64,
    pub truncated: bool,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalCommit {
    pub attempt_id: String,
    pub expected_row_version: u64,
    pub state: AttemptState,
    pub result_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infrastructure_error_digest: Option<String>,
    pub finished_at_ms: u64,
    pub artifacts: Vec<ArtifactRegistration>,
    pub reason_code: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobProjection {
    pub job_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub result_available: bool,
    pub artifacts_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeJobListCursor {
    pub created_at_ms: u64,
    pub job_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeJobListRequest {
    #[serde(default = "default_runtime_list_limit")]
    #[schemars(range(min = 1, max = MAX_RUNTIME_LIST_LIMIT))]
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<RuntimeJobListCursor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeJobSummary {
    pub job_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub client_request_id: String,
    pub workspace_id: String,
    pub source_revision: String,
    pub executable_name: String,
    pub cwd_relative: String,
    pub created_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    pub duration_ms: u64,
    pub result_available: bool,
    pub artifacts_available: bool,
    pub artifact_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeJobListResult {
    pub jobs: Vec<RuntimeJobSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<RuntimeJobListCursor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerIdentity {
    pub boot_id: String,
    pub unit_name: String,
    pub invocation_id: String,
    pub control_group: String,
    pub main_pid: u32,
    pub process_start_identity: String,
    pub runner_start_digest: String,
    pub observed_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConditionUpdate {
    pub condition_type: String,
    pub status: String,
    pub reason_code: String,
    pub evidence_digest: String,
    pub observed_at_ms: u64,
}

pub(crate) fn default_runtime_list_limit() -> u32 {
    20
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UniversalExecutionRequest {
    pub workspace_id: String,
    /// Absolute host path to the executable; PATH lookup is intentionally not performed.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory relative to the Workspace root.
    pub cwd_relative: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskRunRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub client_request_id: String,
    pub principal: String,
    pub global_limit: u32,
    pub execution: UniversalExecutionRequest,
    #[serde(default = "default_task_wait_ms")]
    #[schemars(range(max = MAX_TASK_WAIT_MS))]
    pub wait_ms: u64,
    #[serde(default = "default_task_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stdout_tail_bytes: u64,
    #[serde(default = "default_task_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stderr_tail_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskObserveRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub job_id: String,
    #[serde(default)]
    #[schemars(range(max = MAX_TASK_WAIT_MS))]
    pub wait_ms: u64,
    #[serde(default = "default_task_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stdout_tail_bytes: u64,
    #[serde(default = "default_task_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stderr_tail_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_offset: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskCancelRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub job_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskObservation {
    pub job_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout_tail: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr_tail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_next_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_available_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_eof: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_next_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_available_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_eof: Option<bool>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub stdout_truncated: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub stderr_truncated: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub artifacts_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactReadRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub job_id: String,
    pub artifact_id: String,
    pub offset: u64,
    #[schemars(range(min = 1, max = MAX_ARTIFACT_READ_BYTES))]
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactReadResult {
    pub job_id: String,
    pub artifact_id: String,
    pub content: String,
    pub offset: u64,
    pub next_offset: u64,
    pub eof: bool,
    pub digest: String,
}

pub(crate) fn default_task_wait_ms() -> u64 {
    30_000
}

pub(crate) fn default_task_tail_bytes() -> u64 {
    4096
}

fn is_false(value: &bool) -> bool {
    !*value
}
