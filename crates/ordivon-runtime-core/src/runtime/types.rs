use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{RuntimeError, RuntimeErrorCode, RuntimeResult};

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
                    | Self::Succeeded
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionProfile {
    #[default]
    TrustedLocal,
    ContainedLocal,
}

impl ExecutionProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TrustedLocal => "trusted_local",
            Self::ContainedLocal => "contained_local",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForeignReference {
    pub namespace: String,
    #[serde(rename = "type")]
    pub reference_type: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// Operator-owned root from which Runtime may resolve immutable input objects.
/// This is Core configuration/authority, not Agent-authored path authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputAuthority {
    pub name: String,
    pub root: PathBuf,
}

/// Agent/domain-authored request for one exact object inside a named input authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputBindingRequest {
    pub authority: String,
    pub relative_object: String,
    pub expected_digest: String,
    pub presentation_relative_path: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputAccessMode {
    ReadOnly,
}

/// Concrete immutable input truth frozen into one Runtime execution plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectiveInputBinding {
    pub authority: String,
    pub relative_object: String,
    pub digest: String,
    pub byte_length: u64,
    pub presentation_relative_path: String,
    pub access: InputAccessMode,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub memory_max_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub tasks_max: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub cpu_quota_percent: Option<u32>,
}

impl ExecutionBudget {
    pub fn is_empty(&self) -> bool {
        self.memory_max_bytes.is_none()
            && self.tasks_max.is_none()
            && self.cpu_quota_percent.is_none()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UniversalExecutionStep {
    pub id: String,
    /// Absolute host path to the executable; PATH lookup is intentionally not performed.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory relative to the Workspace root.
    pub cwd_relative: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[schemars(range(min = 1))]
    pub timeout_ms: u64,
    #[serde(default)]
    pub continue_on_error: bool,
}

/// Agent-authored execution step before Runtime resolves omitted mechanical limits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionStepProposal {
    pub id: String,
    /// Absolute host path to the executable; PATH lookup is intentionally not performed.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory relative to the Workspace root.
    pub cwd_relative: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Optional step-local upper bound. Omission delegates only this mechanical limit to Runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub continue_on_error: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeExecutionStep {
    pub id: String,
    pub executable: String,
    pub executable_digest: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub timeout_ms: u64,
    #[serde(default)]
    pub continue_on_error: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeExecutionPlan {
    pub schema_version: u32,
    pub workspace_id: String,
    pub workspace_path: String,
    pub source_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_source_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_git_common_dir: Option<String>,
    pub executable: String,
    pub executable_digest: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[schemars(range(min = 1))]
    pub timeout_ms: u64,
    #[schemars(range(min = 1))]
    pub stdout_limit_bytes: u64,
    #[schemars(range(min = 1))]
    pub stderr_limit_bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<RuntimeExecutionStep>,
    #[serde(default, skip_serializing_if = "ExecutionBudget::is_empty")]
    pub budget: ExecutionBudget,
    #[serde(default)]
    pub execution_profile: ExecutionProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_references: Vec<ForeignReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_set_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effective_inputs: Vec<EffectiveInputBinding>,
    pub principal: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitRequest {
    pub schema_version: u32,
    pub client_request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_identity_digest: Option<String>,
    pub plan: RuntimeExecutionPlan,
    pub global_limit: u32,
}

pub(crate) const REQUEST_IDENTITY_PREFIX: &str = "runtime-request-v1:";
pub(crate) const PROPOSAL_IDENTITY_PREFIX: &str = "runtime-request-v2:";
pub(crate) const INPUT_BOUND_IDENTITY_PREFIX: &str = "runtime-request-input-v1:";
pub(crate) const INPUT_BOUND_PROPOSAL_IDENTITY_PREFIX: &str = "runtime-request-input-v2:";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationRequestIdentity {
    schema_version: u32,
    principal: String,
    workspace_id: String,
    executable: String,
    args: Vec<String>,
    cwd_relative: String,
    env: BTreeMap<String, String>,
    timeout_ms: u64,
    stdout_limit_bytes: u64,
    stderr_limit_bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    steps: Vec<UniversalExecutionStep>,
    budget: ExecutionBudget,
    execution_profile: ExecutionProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    foreign_references: Vec<ForeignReference>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InputBindingIdentity {
    authority: String,
    relative_object: String,
    expected_digest: String,
    presentation_relative_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InputBoundRequestIdentity {
    operation: OperationRequestIdentity,
    inputs: Vec<InputBindingIdentity>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InputBoundProposalIdentity {
    proposal: ProposalRequestIdentity,
    inputs: Vec<InputBindingIdentity>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposalRequestIdentity {
    schema_version: u32,
    principal: String,
    workspace_id: String,
    executable: String,
    args: Vec<String>,
    cwd_relative: String,
    env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout_limit_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr_limit_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    steps: Vec<ExecutionStepProposal>,
    budget: ExecutionBudget,
    execution_profile: ExecutionProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    foreign_references: Vec<ForeignReference>,
}

pub(crate) fn operation_request_identity_digest(request: &TaskRunRequest) -> RuntimeResult<String> {
    operation_request_identity_digest_from_parts(operation_request_identity(request))
}

fn operation_request_identity(request: &TaskRunRequest) -> OperationRequestIdentity {
    OperationRequestIdentity {
        schema_version: request.schema_version,
        principal: request.principal.clone(),
        workspace_id: request.execution.workspace_id.clone(),
        executable: normalize_path_text(&request.execution.executable),
        args: request.execution.args.clone(),
        cwd_relative: normalize_relative_path_text(&request.execution.cwd_relative),
        env: request.execution.env.clone(),
        timeout_ms: request.execution.timeout_ms,
        stdout_limit_bytes: request.execution.stdout_limit_bytes,
        stderr_limit_bytes: request.execution.stderr_limit_bytes,
        steps: request.execution.steps.clone(),
        budget: request.execution.budget.clone(),
        execution_profile: request.execution.execution_profile,
        foreign_references: request.execution.foreign_references.clone(),
    }
}

fn input_binding_identities(inputs: &[InputBindingRequest]) -> Vec<InputBindingIdentity> {
    let mut inputs = inputs
        .iter()
        .map(|input| InputBindingIdentity {
            authority: input.authority.clone(),
            relative_object: input.relative_object.clone(),
            expected_digest: input.expected_digest.to_ascii_lowercase(),
            presentation_relative_path: input.presentation_relative_path.clone(),
        })
        .collect::<Vec<_>>();
    inputs.sort_by(|left, right| {
        (
            &left.presentation_relative_path,
            &left.authority,
            &left.relative_object,
            &left.expected_digest,
        )
            .cmp(&(
                &right.presentation_relative_path,
                &right.authority,
                &right.relative_object,
                &right.expected_digest,
            ))
    });
    inputs
}

pub(crate) fn input_bound_request_identity_digest(
    request: &TaskRunRequest,
    inputs: &[InputBindingRequest],
) -> RuntimeResult<String> {
    let identity = InputBoundRequestIdentity {
        operation: operation_request_identity(request),
        inputs: input_binding_identities(inputs),
    };
    let bytes = serde_json::to_vec(&identity).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            format!("cannot serialize input-bound request identity: {error}"),
            None,
            false,
        )
    })?;
    Ok(format!(
        "{INPUT_BOUND_IDENTITY_PREFIX}{}",
        crate::universal::sha256_bytes(&bytes)
    ))
}

fn proposal_request_identity(proposal: &TaskRunProposal) -> ProposalRequestIdentity {
    ProposalRequestIdentity {
        schema_version: proposal.schema_version,
        principal: proposal.principal.clone(),
        workspace_id: proposal.execution.workspace_id.clone(),
        executable: normalize_path_text(&proposal.execution.executable),
        args: proposal.execution.args.clone(),
        cwd_relative: normalize_relative_path_text(&proposal.execution.cwd_relative),
        env: proposal.execution.env.clone(),
        timeout_ms: proposal.execution.timeout_ms,
        stdout_limit_bytes: proposal.execution.stdout_limit_bytes,
        stderr_limit_bytes: proposal.execution.stderr_limit_bytes,
        steps: proposal
            .execution
            .steps
            .iter()
            .map(|step| ExecutionStepProposal {
                id: step.id.clone(),
                executable: normalize_path_text(&step.executable),
                args: step.args.clone(),
                cwd_relative: normalize_relative_path_text(&step.cwd_relative),
                env: step.env.clone(),
                timeout_ms: step.timeout_ms,
                continue_on_error: step.continue_on_error,
            })
            .collect(),
        budget: proposal.execution.budget.clone(),
        execution_profile: proposal.execution.execution_profile,
        foreign_references: proposal.execution.foreign_references.clone(),
    }
}

pub(crate) fn proposal_request_identity_digest(
    proposal: &TaskRunProposal,
) -> RuntimeResult<String> {
    let bytes = serde_json::to_vec(&proposal_request_identity(proposal)).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            format!("cannot serialize execution proposal identity: {error}"),
            None,
            false,
        )
    })?;
    Ok(format!(
        "{PROPOSAL_IDENTITY_PREFIX}{}",
        crate::universal::sha256_bytes(&bytes)
    ))
}

pub(crate) fn input_bound_proposal_request_identity_digest(
    proposal: &TaskRunProposal,
    inputs: &[InputBindingRequest],
) -> RuntimeResult<String> {
    let identity = InputBoundProposalIdentity {
        proposal: proposal_request_identity(proposal),
        inputs: input_binding_identities(inputs),
    };
    let bytes = serde_json::to_vec(&identity).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            format!("cannot serialize input-bound proposal identity: {error}"),
            None,
            false,
        )
    })?;
    Ok(format!(
        "{INPUT_BOUND_PROPOSAL_IDENTITY_PREFIX}{}",
        crate::universal::sha256_bytes(&bytes)
    ))
}

pub(crate) fn operation_request_identity_digest_from_plan(
    plan: &RuntimeExecutionPlan,
) -> RuntimeResult<String> {
    let cwd = Path::new(&plan.cwd)
        .strip_prefix(&plan.workspace_path)
        .map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "stored execution cwd is outside its Workspace",
                Some("executionPlan"),
                false,
            )
        })?;
    operation_request_identity_digest_from_parts(OperationRequestIdentity {
        schema_version: plan.schema_version,
        principal: plan.principal.clone(),
        workspace_id: plan.workspace_id.clone(),
        executable: normalize_path_text(&plan.executable),
        args: plan.args.clone(),
        cwd_relative: normalize_relative_path_text(&cwd.to_string_lossy()),
        env: plan.env.clone(),
        timeout_ms: plan.timeout_ms,
        stdout_limit_bytes: plan.stdout_limit_bytes,
        stderr_limit_bytes: plan.stderr_limit_bytes,
        steps: plan
            .steps
            .iter()
            .map(|step| UniversalExecutionStep {
                id: step.id.clone(),
                executable: normalize_path_text(&step.executable),
                args: step.args.clone(),
                cwd_relative: Path::new(&step.cwd)
                    .strip_prefix(&plan.workspace_path)
                    .map(|path| normalize_relative_path_text(&path.to_string_lossy()))
                    .unwrap_or_else(|_| step.cwd.clone()),
                env: step.env.clone(),
                timeout_ms: step.timeout_ms,
                continue_on_error: step.continue_on_error,
            })
            .collect(),
        budget: plan.budget.clone(),
        execution_profile: plan.execution_profile,
        foreign_references: plan.foreign_references.clone(),
    })
}

fn operation_request_identity_digest_from_parts(
    identity: OperationRequestIdentity,
) -> RuntimeResult<String> {
    let bytes = serde_json::to_vec(&identity).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            format!("cannot serialize operation request identity: {error}"),
            None,
            false,
        )
    })?;
    Ok(format!(
        "{REQUEST_IDENTITY_PREFIX}{}",
        crate::universal::sha256_bytes(&bytes)
    ))
}

fn normalize_path_text(value: &str) -> String {
    use std::path::Component;

    let path = Path::new(value);
    let absolute = path.is_absolute();
    let mut normalized = if absolute {
        PathBuf::from("/")
    } else {
        PathBuf::new()
    };
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = normalized
                    .file_name()
                    .is_some_and(|name| name != std::ffi::OsStr::new(".."));
                if can_pop {
                    normalized.pop();
                } else if !absolute {
                    normalized.push("..");
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized.to_string_lossy().into_owned()
}

fn normalize_relative_path_text(value: &str) -> String {
    let normalized = normalize_path_text(value);
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
/// Runtime certainty about delivery of the physical execution result.
/// This is not a Task/domain semantic-completion judgment.
pub enum RuntimeDeliveryDisposition {
    /// The Runtime Job has not reached a terminal execution resolution.
    InProgress,
    /// Runtime has committed a conclusive execution result.
    Committed,
    /// Runtime has a result/state that still requires reconciliation before mechanical convergence.
    ReconciliationRequired,
    /// Runtime cannot prove a conclusive execution result; callers must not infer effect-safe redispatch.
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobProjection {
    pub job_id: String,
    /// Compatibility summary only. Use the explicit semantic fields below for control decisions.
    pub status: String,
    /// Persisted Runtime Job intent (`run` or `cancelled`).
    pub desired_state: JobDesiredState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    /// Exact current or latest Runtime Attempt state; never collapsed into queued/working.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_state: Option<AttemptState>,
    /// Exact current/latest Attempt termination intent, including stop and deadline intent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination_intent: Option<AttemptTerminationIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// True only when Runtime has committed a terminal Job resolution.
    pub execution_terminal: bool,
    /// Terminal physical execution resolution; absent while execution is unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_disposition: Option<JobResolution>,
    /// Stable machine reason for the current terminal execution resolution, when recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_reason_code: Option<String>,
    /// Runtime certainty/reconciliation class for the physical result.
    pub delivery_disposition: RuntimeDeliveryDisposition,
    /// True when Runtime requires mechanical recovery/reconciliation for this Job.
    pub recovery_required: bool,
    /// Always false: Runtime does not judge Task/domain semantic completion.
    pub semantic_completion_evaluated: bool,
    /// A Runtime terminal result is durably available; this does not imply semantic completion.
    pub result_available: bool,
    /// True only when at least one registered Runtime Artifact exists for this Job.
    pub artifacts_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeWorkspaceGetRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub workspace_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeWorkspaceListCursor {
    pub created_at_ms: u64,
    pub workspace_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeWorkspaceListRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    #[serde(default = "default_runtime_list_limit")]
    #[schemars(range(min = 1, max = MAX_RUNTIME_LIST_LIMIT))]
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<RuntimeWorkspaceListCursor>,
    #[serde(default)]
    pub include_source_state_digest: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeWorkspaceSummary {
    pub workspace_id: String,
    /// Canonical source repository identity used to create this Workspace.
    pub source_repo: String,
    /// Exact commit from which the Workspace was opened; this is lineage, not the current HEAD.
    pub source_revision: String,
    /// Exact commit currently checked out at Workspace HEAD.
    pub current_head_revision: String,
    pub created_at_ms: u64,
    pub head_mode: String,
    pub dirty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_state_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_job_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeWorkspaceIssueStage {
    Inventory,
    Reconcile,
    ActiveJobs,
    DirtyProbe,
    SourceStateDigest,
    HeadRevision,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeWorkspaceIssue {
    pub workspace_id: String,
    /// Workspace-local projection stage that could not be proven.
    pub stage: RuntimeWorkspaceIssueStage,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeWorkspaceListResult {
    pub workspaces: Vec<RuntimeWorkspaceSummary>,
    /// Stable continuation over healthy Workspace records. Inventory issues are global diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<RuntimeWorkspaceListCursor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<RuntimeWorkspaceIssue>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<String>,
    /// Exact Runtime Workspace identity used to bound Job reattachment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeJobSummary {
    pub job_id: String,
    /// Compatibility summary only. Use the explicit semantic fields below for control decisions.
    pub status: String,
    /// Persisted Runtime Job intent (`run` or `cancelled`).
    pub desired_state: JobDesiredState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    /// Exact current or latest Runtime Attempt state; never collapsed into queued/working.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_state: Option<AttemptState>,
    /// Exact current/latest Attempt termination intent, including stop and deadline intent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination_intent: Option<AttemptTerminationIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// True only when Runtime has committed a terminal Job resolution.
    pub execution_terminal: bool,
    /// Terminal physical execution resolution; absent while execution is unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_disposition: Option<JobResolution>,
    /// Stable machine reason for the current terminal execution resolution, when recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_reason_code: Option<String>,
    /// Runtime certainty/reconciliation class for the physical result.
    pub delivery_disposition: RuntimeDeliveryDisposition,
    /// True when Runtime requires mechanical recovery/reconciliation for this Job.
    pub recovery_required: bool,
    /// Always false: Runtime does not judge Task/domain semantic completion.
    pub semantic_completion_evaluated: bool,
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
    /// A Runtime terminal result is durably available; this does not imply semantic completion.
    pub result_available: bool,
    /// True only when at least one registered Runtime Artifact exists for this Job.
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
pub struct RuntimeInvariantViolation {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    pub detail: String,
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

/// Agent-authored execution proposal. Action fields are concrete; only proven mechanical
/// execution limits may be omitted and resolved by Runtime at new admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionProposal {
    pub workspace_id: String,
    /// Absolute host path to the executable; PATH lookup is intentionally not performed.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory relative to the Workspace root.
    pub cwd_relative: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub stdout_limit_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub stderr_limit_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ExecutionStepProposal>,
    #[serde(default, skip_serializing_if = "ExecutionBudget::is_empty")]
    pub budget: ExecutionBudget,
    #[serde(default)]
    pub execution_profile: ExecutionProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_references: Vec<ForeignReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskRunProposal {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub client_request_id: String,
    pub principal: String,
    pub global_limit: u32,
    pub execution: ExecutionProposal,
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
    /// Optional ordered fail-fast steps. Empty preserves raw single-command execution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<UniversalExecutionStep>,
    #[serde(default, skip_serializing_if = "ExecutionBudget::is_empty")]
    pub budget: ExecutionBudget,
    #[serde(default)]
    pub execution_profile: ExecutionProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_references: Vec<ForeignReference>,
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskObserveWaitUntil {
    #[default]
    Terminal,
    ChangeOrTerminal,
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
    #[serde(default)]
    pub wait_until: TaskObserveWaitUntil,
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
pub struct EffectiveStepTimeout {
    pub id: String,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectiveExecutionLimits {
    pub timeout_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub step_timeouts: Vec<EffectiveStepTimeout>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskObservation {
    pub job_id: String,
    /// Compatibility summary only. Use the explicit semantic fields below for control decisions.
    pub status: String,
    /// Persisted Runtime Job intent (`run` or `cancelled`).
    pub desired_state: JobDesiredState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    /// Exact current or latest Runtime Attempt state; never collapsed into queued/working.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_state: Option<AttemptState>,
    /// Exact current/latest Attempt termination intent, including stop and deadline intent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination_intent: Option<AttemptTerminationIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// True only when Runtime has committed a terminal Job resolution.
    pub execution_terminal: bool,
    /// Terminal physical execution resolution; absent while execution is unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_disposition: Option<JobResolution>,
    /// Stable machine reason for the current terminal execution resolution, when recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_reason_code: Option<String>,
    /// Runtime certainty/reconciliation class for the physical result.
    pub delivery_disposition: RuntimeDeliveryDisposition,
    /// Durable effective execution limits frozen into this Job at admission.
    pub effective_limits: EffectiveExecutionLimits,
    /// True when Runtime requires mechanical recovery/reconciliation for this Job.
    pub recovery_required: bool,
    /// Always false: Runtime does not judge Task/domain semantic completion.
    pub semantic_completion_evaluated: bool,
    /// A Runtime terminal result is durably available; this does not imply semantic completion.
    pub result_available: bool,
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
    /// True only when at least one registered Runtime Artifact exists for this Job.
    pub artifacts_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_output_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step_elapsed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_step_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_step_index: Option<u32>,
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
