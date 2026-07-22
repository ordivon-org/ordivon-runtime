use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{M6Error, M6Result};

pub const M6_SCHEMA_VERSION: u32 = 1;
pub const MAX_M6_LIST_LIMIT: u32 = 100;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    GovernedProfile,
    UniversalSandbox,
}

impl PlanKind {
    pub(crate) fn as_db(self) -> &'static str {
        match self {
            Self::GovernedProfile => "governed_profile",
            Self::UniversalSandbox => "universal_sandbox",
        }
    }

    pub(crate) fn parse(value: &str) -> M6Result<Self> {
        match value {
            "governed_profile" => Ok(Self::GovernedProfile),
            "universal_sandbox" => Ok(Self::UniversalSandbox),
            _ => Err(M6Error::invalid("unknown plan kind", "planKind")),
        }
    }
}

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

    pub(crate) fn parse(value: &str) -> M6Result<Self> {
        match value {
            "run" => Ok(Self::Run),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(M6Error::invalid("unknown desired state", "desiredState")),
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

    pub(crate) fn parse(value: &str) -> M6Result<Self> {
        match value {
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "timed_out" => Ok(Self::TimedOut),
            "cancelled" => Ok(Self::Cancelled),
            "lost" => Ok(Self::Lost),
            "orphaned" => Ok(Self::Orphaned),
            _ => Err(M6Error::invalid("unknown job resolution", "resolution")),
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

    pub(crate) fn parse(value: &str) -> M6Result<Self> {
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
            _ => Err(M6Error::invalid("unknown attempt state", "state")),
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
pub enum M6TerminationIntent {
    Natural,
    StopRequested,
    DeadlineExceeded,
}

impl M6TerminationIntent {
    pub(crate) fn as_db(self) -> &'static str {
        match self {
            Self::Natural => "natural",
            Self::StopRequested => "stop_requested",
            Self::DeadlineExceeded => "deadline_exceeded",
        }
    }

    pub(crate) fn parse(value: &str) -> M6Result<Self> {
        match value {
            "natural" => Ok(Self::Natural),
            "stop_requested" => Ok(Self::StopRequested),
            "deadline_exceeded" => Ok(Self::DeadlineExceeded),
            _ => Err(M6Error::invalid(
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

    pub(crate) fn parse(value: &str) -> M6Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "held_orphaned" => Ok(Self::HeldOrphaned),
            "released" => Ok(Self::Released),
            _ => Err(M6Error::invalid(
                "unknown reservation state",
                "reservationState",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6ExecutionPlan {
    pub schema_version: u32,
    pub plan_kind: PlanKind,
    pub workspace_id: String,
    pub workspace_path: String,
    pub source_revision: String,
    pub executable: String,
    pub executable_digest: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
    pub policy_id: String,
    pub policy_version: String,
    pub policy_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub principal: String,
    pub authority_ref: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6SubmitRequest {
    pub schema_version: u32,
    pub client_request_id: String,
    pub plan: M6ExecutionPlan,
    pub global_limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_limit: Option<u32>,
    #[cfg(feature = "runtime-hardening-m7")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_quota: Option<crate::M7AdmissionQuota>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobRecordM6 {
    pub job_id: String,
    pub principal: String,
    pub client_request_id: String,
    pub request_digest: String,
    pub operation_digest: String,
    pub workspace_id: String,
    pub workspace_snapshot_json: String,
    pub plan_kind: PlanKind,
    pub execution_plan_json: String,
    pub execution_plan_digest: String,
    pub policy_id: String,
    pub policy_version: String,
    pub policy_digest: String,
    pub authority_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
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
pub struct AttemptRecordM6 {
    pub attempt_id: String,
    pub job_id: String,
    pub attempt_number: u32,
    pub state: AttemptState,
    pub termination_intent: M6TerminationIntent,
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
pub struct ReservationRecordM6 {
    pub reservation_id: String,
    pub attempt_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub global_limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_limit: Option<u32>,
    pub state: ReservationState,
    pub acquired_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub released_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatedAdmissionM6 {
    pub job: JobRecordM6,
    pub attempt: AttemptRecordM6,
    pub reservation: ReservationRecordM6,
    pub launch_token: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum AdmissionOutcomeM6 {
    Created(Box<CreatedAdmissionM6>),
    Existing { job: Box<JobRecordM6> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactRegistrationM6 {
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
pub struct ArtifactRecordM6 {
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
pub struct TerminalCommitM6 {
    pub attempt_id: String,
    pub expected_row_version: u64,
    pub state: AttemptState,
    pub result_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infrastructure_error_digest: Option<String>,
    pub finished_at_ms: u64,
    pub artifacts: Vec<ArtifactRegistrationM6>,
    pub reason_code: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobProjectionM6 {
    pub job_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub result_available: bool,
    pub artifacts_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobListCursorM6 {
    pub created_at_ms: u64,
    pub job_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobListRequestM6 {
    #[serde(default = "default_m6_list_limit")]
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<JobListCursorM6>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobListResultM6 {
    pub jobs: Vec<JobProjectionM6>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<JobListCursorM6>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerIdentityM6 {
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
pub struct ConditionUpdateM6 {
    pub condition_type: String,
    pub status: String,
    pub reason_code: String,
    pub evidence_digest: String,
    pub observed_at_ms: u64,
}

pub(crate) fn default_m6_list_limit() -> u32 {
    50
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6UniversalExecutionRequest {
    pub workspace_id: String,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd_relative: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6TaskRunRequest {
    pub schema_version: u32,
    pub client_request_id: String,
    pub principal: String,
    pub authority_ref: String,
    pub policy_id: String,
    pub policy_version: String,
    pub policy_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub global_limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_limit: Option<u32>,
    pub execution: M6UniversalExecutionRequest,
    #[serde(default = "default_m6_wait_ms")]
    pub wait_ms: u64,
    #[serde(default = "default_m6_tail_bytes")]
    pub stdout_tail_bytes: u64,
    #[serde(default = "default_m6_tail_bytes")]
    pub stderr_tail_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6TaskObserveRequest {
    pub schema_version: u32,
    pub job_id: String,
    #[serde(default)]
    pub wait_ms: u64,
    #[serde(default = "default_m6_tail_bytes")]
    pub stdout_tail_bytes: u64,
    #[serde(default = "default_m6_tail_bytes")]
    pub stderr_tail_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6TaskCancelRequest {
    pub schema_version: u32,
    pub job_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6TaskObservation {
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub stdout_truncated: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub stderr_truncated: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub artifacts_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6ArtifactReadRequest {
    pub schema_version: u32,
    pub job_id: String,
    pub artifact_id: String,
    pub offset: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6ArtifactReadResult {
    pub job_id: String,
    pub artifact_id: String,
    pub content: String,
    pub offset: u64,
    pub next_offset: u64,
    pub eof: bool,
    pub digest: String,
}

pub(crate) fn default_m6_wait_ms() -> u64 {
    30_000
}

pub(crate) fn default_m6_tail_bytes() -> u64 {
    4096
}

fn is_false(value: &bool) -> bool {
    !*value
}
