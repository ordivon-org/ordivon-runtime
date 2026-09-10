use std::borrow::Cow;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ordivon_runtime_core::{
    read_workspace_content, read_workspace_slice_compact, read_workspace_text_compact,
    workspace_changes_page, workspace_diff_compact, ArtifactReadRequest, ArtifactReadResult,
    CompactWorkspaceDiffResult, CompactWorkspaceOpenResult, DurableWorkspacePatchRequest,
    DurableWorkspacePatchResult, ExecutionBudget, ExecutionProfile, ExecutionProposal,
    ExecutionStepProposal, ExecutionTarget, ForeignReference, GitWorkspaceCreateRequest,
    HostDependencyBinding, InputAuthority, InputBindingRequest, Runtime, RuntimeCapabilities,
    RuntimeCapacity, RuntimeConfig, RuntimeError, RuntimeExecutionTargetCapability,
    RuntimeJobInspection, RuntimeJobListRequest, RuntimeJobListResult, RuntimeReleaseAdmission,
    RuntimeReleaseGetRequest, RuntimeReleaseProjection, RuntimeReleaseRequest,
    RuntimeWorkspaceGetRequest, RuntimeWorkspaceListRequest, RuntimeWorkspaceListResult,
    RuntimeWorkspaceSummary, TaskCancelRequest, TaskObservation, TaskObserveRequest,
    TaskRunProposal, TaskRunRequest, UniversalExecError, UniversalExecutionRequest,
    UniversalExecutionStep, UniversalExecutorConfig, WindowsAuthority, WorkspaceChangeCursor,
    WorkspaceChangePageRequest as ExecWorkspaceChangePageRequest, WorkspaceChangePageResult,
    WorkspaceCloseRequest, WorkspaceCloseResult, WorkspaceContentMetadata, WorkspaceContentRequest,
    WorkspaceDiffRequest as ExecWorkspaceDiffRequest, WorkspaceFilePatch, WorkspaceMutateRequest,
    WorkspaceMutateResult, WorkspacePatchOperationStatus, WorkspacePatchRequest,
    WorkspacePatchStatusRequest, WorkspaceReadRequest as ExecWorkspaceReadRequest,
    WorkspaceReadSliceRequest, CLIENT_REQUEST_ID_MAX_LENGTH, CLIENT_REQUEST_ID_MIN_LENGTH,
    CLIENT_REQUEST_ID_PATTERN, DEFAULT_INSPECTION_EVENT_LIMIT, ENVIRONMENT_VARIABLE_NAME_PATTERN,
    MAX_INSPECTION_EVENT_LIMIT, MAX_TASK_TAIL_BYTES, MAX_TASK_WAIT_MS,
    MAX_WORKSPACE_CHANGE_PAGE_ENTRIES, MAX_WORKSPACE_IO_BYTES, RUNTIME_SCHEMA_VERSION,
    WORKSPACE_ID_MAX_LENGTH, WORKSPACE_ID_MIN_LENGTH, WORKSPACE_ID_PATTERN,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::{IntoCallToolResult, ToolCallContext};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_router, ErrorData as McpError, ServerHandler};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[cfg(test)]
use ordivon_runtime_core::{LOGICAL_ID_MAX_LENGTH, LOGICAL_ID_MIN_LENGTH, LOGICAL_ID_PATTERN};

use crate::{append_rotating_jsonl, DEFAULT_TRACE_ROTATION_BYTES};

static GLOBAL_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static GLOBAL_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Pinned MCP tool-input schema version. Omitted `schemaVersion` defaults to
/// the current pinned version so external clients (which do not read the
/// `const` pin) can call tools without carrying an internal version field;
/// explicit non-pinned values are still rejected by handlers.
fn default_schema_version() -> u32 {
    1
}

#[allow(dead_code)]
#[derive(JsonSchema)]
#[schemars(extend("pattern" = ENVIRONMENT_VARIABLE_NAME_PATTERN))]
struct EnvironmentVariableNameSchema(String);

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceOpenRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = WORKSPACE_ID_MIN_LENGTH, max = WORKSPACE_ID_MAX_LENGTH), regex(pattern = WORKSPACE_ID_PATTERN))]
    pub workspace_id: Option<String>,
    pub source_repo: String,
    pub source_revision: String,
}

impl WorkspaceOpenRequest {
    fn bind(self) -> GitWorkspaceCreateRequest {
        GitWorkspaceCreateRequest {
            schema_version: self.schema_version,
            workspace_id: self
                .workspace_id
                .unwrap_or_else(|| format!("ws-{}", Uuid::now_v7())),
            source_repo: self.source_repo,
            source_revision: self.source_revision,
        }
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkspaceReadMode {
    Full,
    Slice,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceReadRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = WORKSPACE_ID_MIN_LENGTH, max = WORKSPACE_ID_MAX_LENGTH), regex(pattern = WORKSPACE_ID_PATTERN))]
    pub workspace_id: String,
    pub relative_path: String,
    pub mode: WorkspaceReadMode,
    #[serde(default)]
    pub offset: u64,
    #[schemars(range(min = 1, max = MAX_WORKSPACE_IO_BYTES))]
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReadResult {
    pub content: String,
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_byte_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eof: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceDiffRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = WORKSPACE_ID_MIN_LENGTH, max = WORKSPACE_ID_MAX_LENGTH), regex(pattern = WORKSPACE_ID_PATTERN))]
    pub workspace_id: String,
    #[schemars(range(min = 1, max = MAX_WORKSPACE_IO_BYTES))]
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskGetRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub job_id: String,
    #[serde(default = "default_task_get_event_limit")]
    #[schemars(range(min = 1, max = MAX_INSPECTION_EVENT_LIMIT))]
    pub event_limit: u32,
}

fn default_task_get_event_limit() -> u32 {
    DEFAULT_INSPECTION_EVENT_LIMIT
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeDescribeRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDescribeResult {
    pub schema_version: u32,
    pub global_execution_limit: u32,
    pub max_runtime_ms: u64,
    pub max_output_bytes: u64,
    pub allowed_executable_roots: Vec<String>,
    pub input_authorities: Vec<String>,
    pub input_ingress_authorities: Vec<String>,
    pub targets: Vec<RuntimeExecutionTargetCapability>,
    pub structured_release_configured: bool,
}

impl RuntimeDescribeResult {
    fn from_capabilities(
        capabilities: RuntimeCapabilities,
        global_execution_limit: u32,
        structured_release_configured: bool,
        input_ingress_authorities: Vec<String>,
    ) -> Self {
        Self {
            schema_version: capabilities.schema_version,
            global_execution_limit,
            max_runtime_ms: capabilities.max_runtime_ms,
            max_output_bytes: capabilities.max_output_bytes,
            allowed_executable_roots: capabilities.allowed_executable_roots,
            input_authorities: capabilities.input_authorities,
            input_ingress_authorities,
            targets: capabilities.targets,
            structured_release_configured,
        }
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeReleaseApplyToolRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = CLIENT_REQUEST_ID_MIN_LENGTH, max = CLIENT_REQUEST_ID_MAX_LENGTH), extend("pattern" = CLIENT_REQUEST_ID_PATTERN))]
    pub client_request_id: String,
    #[schemars(length(min = WORKSPACE_ID_MIN_LENGTH, max = WORKSPACE_ID_MAX_LENGTH), regex(pattern = WORKSPACE_ID_PATTERN))]
    pub workspace_id: String,
    pub commit: String,
    pub candidate_manifest_digest: String,
    #[schemars(range(min = 1))]
    pub expected_tool_count: u32,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeReleaseGetToolRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = CLIENT_REQUEST_ID_MIN_LENGTH, max = CLIENT_REQUEST_ID_MAX_LENGTH), extend("pattern" = CLIENT_REQUEST_ID_PATTERN))]
    pub client_request_id: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputIngressFilePayload {
    #[serde(rename = "download_url")]
    #[schemars(rename = "download_url")]
    pub download_url: String,
    #[serde(rename = "file_id")]
    #[schemars(rename = "file_id")]
    pub file_id: String,
    #[serde(default, rename = "file_name", skip_serializing_if = "Option::is_none")]
    #[schemars(rename = "file_name")]
    pub file_name: Option<String>,
    #[serde(default, rename = "mime_type", skip_serializing_if = "Option::is_none")]
    #[schemars(rename = "mime_type")]
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputIngressToolRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = CLIENT_REQUEST_ID_MIN_LENGTH, max = CLIENT_REQUEST_ID_MAX_LENGTH), extend("pattern" = CLIENT_REQUEST_ID_PATTERN))]
    pub client_request_id: String,
    pub authority: String,
    pub relative_object: String,
    pub expected_sha256: String,
    #[schemars(range(min = 1))]
    pub expected_size_bytes: u64,
    pub file: InputIngressFilePayload,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InputIngressToolResult {
    pub schema_version: u32,
    pub kind: String,
    pub authority: String,
    pub relative_object: String,
    pub expected_sha256: String,
    pub expected_size_bytes: u64,
    pub host_file_reference_digest: String,
    pub source_identity_standing: String,
    pub workstation_receipt: serde_json::Value,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct InputIngressExecutionConfig {
    pub staging_root: PathBuf,
    pub workstation_tool: PathBuf,
    pub workstation_config: PathBuf,
    pub workstation_carrier: String,
    pub authorities: Vec<String>,
    pub download_hosts: Vec<String>,
    pub max_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct RuntimeReleaseExecutionConfig {
    pub source_repo: PathBuf,
    pub install_dir: PathBuf,
    pub database: PathBuf,
    pub env_file: PathBuf,
    pub receipt_root: PathBuf,
    pub required_ref: String,
    pub timeout_ms: u64,
}

impl RuntimeReleaseExecutionConfig {
    fn candidate_dir(&self, commit: &str) -> PathBuf {
        self.source_repo
            .join("target")
            .join("ordivon-release-candidates")
            .join(commit)
            .join("release")
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceChangesRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = WORKSPACE_ID_MIN_LENGTH, max = WORKSPACE_ID_MAX_LENGTH), regex(pattern = WORKSPACE_ID_PATTERN))]
    pub workspace_id: String,
    #[serde(default = "default_change_page_limit")]
    #[schemars(range(min = 1, max = MAX_WORKSPACE_CHANGE_PAGE_ENTRIES))]
    pub limit: u32,
    #[serde(default = "default_change_page_bytes")]
    #[schemars(range(min = 1, max = MAX_WORKSPACE_IO_BYTES))]
    pub max_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<WorkspaceChangeCursor>,
}

fn default_change_page_limit() -> u32 {
    64
}

fn default_change_page_bytes() -> u64 {
    256 * 1024
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePatchToolRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = CLIENT_REQUEST_ID_MIN_LENGTH, max = CLIENT_REQUEST_ID_MAX_LENGTH), extend("pattern" = CLIENT_REQUEST_ID_PATTERN))]
    pub client_request_id: String,
    #[schemars(length(min = WORKSPACE_ID_MIN_LENGTH, max = WORKSPACE_ID_MAX_LENGTH), regex(pattern = WORKSPACE_ID_PATTERN))]
    pub workspace_id: String,
    #[schemars(length(min = 1))]
    pub files: Vec<WorkspaceFilePatch>,
    #[serde(default = "default_patch_diff_bytes")]
    #[schemars(range(min = 1, max = MAX_WORKSPACE_IO_BYTES))]
    pub max_diff_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePatchStatusToolRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = CLIENT_REQUEST_ID_MIN_LENGTH, max = CLIENT_REQUEST_ID_MAX_LENGTH), extend("pattern" = CLIENT_REQUEST_ID_PATTERN))]
    pub client_request_id: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = CLIENT_REQUEST_ID_MIN_LENGTH, max = CLIENT_REQUEST_ID_MAX_LENGTH), extend("pattern" = CLIENT_REQUEST_ID_PATTERN))]
    pub client_request_id: String,
    pub execution: ExecutionProposal,
    /// Maximum time this MCP call waits for observation before returning. This does not
    /// change the Job execution timeout; use `execution.timeoutMs` for that limit.
    #[serde(default = "default_exec_wait_ms")]
    #[schemars(range(max = MAX_TASK_WAIT_MS))]
    pub wait_ms: u64,
    /// Maximum retained stdout bytes included in this MCP response tail (0..=65536).
    /// This does not change Job output retention; use `execution.stdoutLimitBytes` for that.
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stdout_tail_bytes: u64,
    /// Maximum retained stderr bytes included in this MCP response tail (0..=65536).
    /// This does not change Job output retention; use `execution.stderrLimitBytes` for that.
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stderr_tail_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecBoundExecution {
    #[schemars(length(min = WORKSPACE_ID_MIN_LENGTH, max = WORKSPACE_ID_MAX_LENGTH), regex(pattern = WORKSPACE_ID_PATTERN))]
    pub workspace_id: String,
    /// Absolute host path to the executable; PATH lookup is intentionally not performed.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory relative to the Workspace root.
    pub cwd_relative: String,
    #[serde(default)]
    #[schemars(with = "BTreeMap<EnvironmentVariableNameSchema, String>")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_ms: Option<u64>,
    /// Maximum stdout bytes Runtime retains for the Job/Attempt. This is durable output
    /// capture, not the response tail returned by the admitting MCP call; callers tune that
    /// separately with top-level `stdoutTailBytes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub stdout_limit_bytes: Option<u64>,
    /// Maximum stderr bytes Runtime retains for the Job/Attempt. This is durable output
    /// capture, not the response tail returned by the admitting MCP call; callers tune that
    /// separately with top-level `stderrTailBytes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub stderr_limit_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ExecutionStepProposal>,
    #[serde(default, skip_serializing_if = "ExecutionBudget::is_empty")]
    pub budget: ExecutionBudget,
    #[serde(default)]
    pub execution_target: ExecutionTarget,
    #[serde(default)]
    pub windows_authority: WindowsAuthority,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_references: Vec<ForeignReference>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecBoundRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = CLIENT_REQUEST_ID_MIN_LENGTH, max = CLIENT_REQUEST_ID_MAX_LENGTH), extend("pattern" = CLIENT_REQUEST_ID_PATTERN))]
    pub client_request_id: String,
    pub execution: WorkspaceExecBoundExecution,
    #[schemars(length(min = 1))]
    pub inputs: Vec<InputBindingRequest>,
    /// Maximum time this MCP call waits for observation before returning. This does not
    /// change the Job execution timeout; use `execution.timeoutMs` for that limit.
    #[serde(default = "default_exec_wait_ms")]
    #[schemars(range(max = MAX_TASK_WAIT_MS))]
    pub wait_ms: u64,
    /// Maximum retained stdout bytes included in this MCP response tail (0..=65536).
    /// This does not change Job output retention; use `execution.stdoutLimitBytes` for that.
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stdout_tail_bytes: u64,
    /// Maximum retained stderr bytes included in this MCP response tail (0..=65536).
    /// This does not change Job output retention; use `execution.stderrLimitBytes` for that.
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stderr_tail_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecPlanInput {
    #[schemars(length(min = WORKSPACE_ID_MIN_LENGTH, max = WORKSPACE_ID_MAX_LENGTH), regex(pattern = WORKSPACE_ID_PATTERN))]
    pub workspace_id: String,
    #[schemars(length(min = 1))]
    pub steps: Vec<ExecutionStepProposal>,
    /// Optional Job-wide deadline. The fully explicit legacy request shape preserves its
    /// historical step-sum identity; every proposal-shaped omission delegates to Runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_ms: Option<u64>,
    /// Maximum stdout bytes Runtime retains for the Job/Attempt. This is durable output
    /// capture, not the response tail returned by the admitting MCP call; callers tune that
    /// separately with top-level `stdoutTailBytes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub stdout_limit_bytes: Option<u64>,
    /// Maximum stderr bytes Runtime retains for the Job/Attempt. This is durable output
    /// capture, not the response tail returned by the admitting MCP call; callers tune that
    /// separately with top-level `stderrTailBytes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub stderr_limit_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "ExecutionBudget::is_empty")]
    pub budget: ExecutionBudget,
    #[serde(default)]
    pub execution_profile: ExecutionProfile,
    #[serde(default)]
    pub execution_target: ExecutionTarget,
    #[serde(default)]
    pub windows_authority: WindowsAuthority,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_references: Vec<ForeignReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_dependencies: Vec<HostDependencyBinding>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecPlanRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[schemars(length(min = CLIENT_REQUEST_ID_MIN_LENGTH, max = CLIENT_REQUEST_ID_MAX_LENGTH), extend("pattern" = CLIENT_REQUEST_ID_PATTERN))]
    pub client_request_id: String,
    pub execution: WorkspaceExecPlanInput,
    /// Maximum time this MCP call waits for observation before returning. This does not
    /// change the Job execution timeout; use `execution.timeoutMs` for that limit.
    #[serde(default = "default_exec_wait_ms")]
    #[schemars(range(max = MAX_TASK_WAIT_MS))]
    pub wait_ms: u64,
    /// Maximum retained stdout bytes included in this MCP response tail (0..=65536).
    /// This does not change Job output retention; use `execution.stdoutLimitBytes` for that.
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stdout_tail_bytes: u64,
    /// Maximum retained stderr bytes included in this MCP response tail (0..=65536).
    /// This does not change Job output retention; use `execution.stderrLimitBytes` for that.
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stderr_tail_bytes: u64,
}

enum BoundTaskRun {
    Legacy(TaskRunRequest),
    Proposal(TaskRunProposal),
}

#[derive(Clone)]
pub struct ExecutionContext {
    pub principal: String,
    pub global_limit: u32,
}

impl ExecutionContext {
    fn bind_patch(&self, request: WorkspacePatchToolRequest) -> DurableWorkspacePatchRequest {
        DurableWorkspacePatchRequest {
            schema_version: request.schema_version,
            principal: self.principal.clone(),
            client_request_id: request.client_request_id,
            patch: WorkspacePatchRequest {
                schema_version: request.schema_version,
                workspace_id: request.workspace_id,
                files: request.files,
                max_diff_bytes: request.max_diff_bytes,
            },
        }
    }

    fn bind_patch_status(
        &self,
        request: WorkspacePatchStatusToolRequest,
    ) -> WorkspacePatchStatusRequest {
        WorkspacePatchStatusRequest {
            schema_version: request.schema_version,
            principal: self.principal.clone(),
            client_request_id: request.client_request_id,
        }
    }

    fn bind(&self, request: WorkspaceExecRequest) -> BoundTaskRun {
        let legacy_compatible = request.execution.timeout_ms.is_some()
            && request.execution.stdout_limit_bytes.is_some()
            && request.execution.stderr_limit_bytes.is_some()
            && request
                .execution
                .steps
                .iter()
                .all(|step| step.timeout_ms.is_some());
        if legacy_compatible {
            BoundTaskRun::Legacy(TaskRunRequest {
                schema_version: request.schema_version,
                client_request_id: request.client_request_id,
                principal: self.principal.clone(),
                global_limit: self.global_limit,
                execution: UniversalExecutionRequest {
                    workspace_id: request.execution.workspace_id,
                    executable: request.execution.executable,
                    args: request.execution.args,
                    cwd_relative: request.execution.cwd_relative,
                    env: request.execution.env,
                    timeout_ms: request.execution.timeout_ms.expect("checked explicit"),
                    stdout_limit_bytes: request
                        .execution
                        .stdout_limit_bytes
                        .expect("checked explicit"),
                    stderr_limit_bytes: request
                        .execution
                        .stderr_limit_bytes
                        .expect("checked explicit"),
                    steps: request
                        .execution
                        .steps
                        .into_iter()
                        .map(|step| UniversalExecutionStep {
                            id: step.id,
                            executable: step.executable,
                            args: step.args,
                            cwd_relative: step.cwd_relative,
                            env: step.env,
                            timeout_ms: step.timeout_ms.expect("checked explicit"),
                            continue_on_error: step.continue_on_error,
                        })
                        .collect(),
                    budget: request.execution.budget,
                    execution_profile: request.execution.execution_profile,
                    execution_target: request.execution.execution_target,
                    windows_authority: request.execution.windows_authority,
                    foreign_references: request.execution.foreign_references,
                    host_dependencies: request.execution.host_dependencies,
                },
                wait_ms: request.wait_ms,
                stdout_tail_bytes: request.stdout_tail_bytes,
                stderr_tail_bytes: request.stderr_tail_bytes,
            })
        } else {
            BoundTaskRun::Proposal(TaskRunProposal {
                schema_version: request.schema_version,
                client_request_id: request.client_request_id,
                principal: self.principal.clone(),
                global_limit: self.global_limit,
                execution: request.execution,
                wait_ms: request.wait_ms,
                stdout_tail_bytes: request.stdout_tail_bytes,
                stderr_tail_bytes: request.stderr_tail_bytes,
            })
        }
    }

    fn bind_bound(
        &self,
        request: WorkspaceExecBoundRequest,
    ) -> (TaskRunProposal, Vec<InputBindingRequest>) {
        let execution = request.execution;
        (
            TaskRunProposal {
                schema_version: request.schema_version,
                client_request_id: request.client_request_id,
                principal: self.principal.clone(),
                global_limit: self.global_limit,
                execution: ExecutionProposal {
                    workspace_id: execution.workspace_id,
                    executable: execution.executable,
                    args: execution.args,
                    cwd_relative: execution.cwd_relative,
                    env: execution.env,
                    timeout_ms: execution.timeout_ms,
                    stdout_limit_bytes: execution.stdout_limit_bytes,
                    stderr_limit_bytes: execution.stderr_limit_bytes,
                    steps: execution.steps,
                    budget: execution.budget,
                    execution_profile: match execution.execution_target {
                        ExecutionTarget::LocalLinux => ExecutionProfile::ContainedLocal,
                        ExecutionTarget::WindowsNative => ExecutionProfile::TrustedLocal,
                    },
                    execution_target: execution.execution_target,
                    windows_authority: execution.windows_authority,
                    foreign_references: execution.foreign_references,
                    host_dependencies: Vec::new(),
                },
                wait_ms: request.wait_ms,
                stdout_tail_bytes: request.stdout_tail_bytes,
                stderr_tail_bytes: request.stderr_tail_bytes,
            },
            request.inputs,
        )
    }

    fn bind_bound_trusted(
        &self,
        request: WorkspaceExecBoundRequest,
    ) -> Result<(TaskRunProposal, Vec<InputBindingRequest>), ToolError> {
        if request.execution.execution_target != ExecutionTarget::LocalLinux {
            return Err(ToolError::invalid(
                "workspace.execBoundTrusted supports local_linux only",
                "execution.executionTarget",
            ));
        }
        if request.execution.windows_authority != WindowsAuthority::Limited {
            return Err(ToolError::invalid(
                "workspace.execBoundTrusted does not accept Windows authority selection",
                "execution.windowsAuthority",
            ));
        }
        let execution = request.execution;
        Ok((
            TaskRunProposal {
                schema_version: request.schema_version,
                client_request_id: request.client_request_id,
                principal: self.principal.clone(),
                global_limit: self.global_limit,
                execution: ExecutionProposal {
                    workspace_id: execution.workspace_id,
                    executable: execution.executable,
                    args: execution.args,
                    cwd_relative: execution.cwd_relative,
                    env: execution.env,
                    timeout_ms: execution.timeout_ms,
                    stdout_limit_bytes: execution.stdout_limit_bytes,
                    stderr_limit_bytes: execution.stderr_limit_bytes,
                    steps: execution.steps,
                    budget: execution.budget,
                    execution_profile: ExecutionProfile::TrustedLocal,
                    execution_target: ExecutionTarget::LocalLinux,
                    windows_authority: WindowsAuthority::Limited,
                    foreign_references: execution.foreign_references,
                    host_dependencies: Vec::new(),
                },
                wait_ms: request.wait_ms,
                stdout_tail_bytes: request.stdout_tail_bytes,
                stderr_tail_bytes: request.stderr_tail_bytes,
            },
            request.inputs,
        ))
    }

    fn bind_plan(&self, request: WorkspaceExecPlanRequest) -> Result<BoundTaskRun, ToolError> {
        let first = request.execution.steps.first().cloned().ok_or_else(|| {
            ToolError::invalid("steps must contain at least one item", "execution.steps")
        })?;
        let all_step_timeouts_explicit = request
            .execution
            .steps
            .iter()
            .all(|step| step.timeout_ms.is_some());
        let legacy_compatible = request.execution.timeout_ms.is_none()
            && all_step_timeouts_explicit
            && request.execution.stdout_limit_bytes.is_some()
            && request.execution.stderr_limit_bytes.is_some();
        if legacy_compatible {
            // Compatibility only: v1 execPlan identity historically derived its overall timeout
            // from the explicit step sum. This arithmetic is not a Runtime execution law.
            let timeout_ms = request
                .execution
                .steps
                .iter()
                .try_fold(0_u64, |total, step| {
                    total.checked_add(step.timeout_ms.expect("checked explicit"))
                })
                .ok_or_else(|| {
                    ToolError::invalid("step timeout sum overflowed", "execution.steps")
                })?;
            return Ok(BoundTaskRun::Legacy(TaskRunRequest {
                schema_version: request.schema_version,
                client_request_id: request.client_request_id,
                principal: self.principal.clone(),
                global_limit: self.global_limit,
                execution: UniversalExecutionRequest {
                    workspace_id: request.execution.workspace_id,
                    executable: first.executable.clone(),
                    args: first.args.clone(),
                    cwd_relative: first.cwd_relative.clone(),
                    env: first.env.clone(),
                    timeout_ms,
                    stdout_limit_bytes: request
                        .execution
                        .stdout_limit_bytes
                        .expect("checked explicit"),
                    stderr_limit_bytes: request
                        .execution
                        .stderr_limit_bytes
                        .expect("checked explicit"),
                    steps: request
                        .execution
                        .steps
                        .into_iter()
                        .map(|step| UniversalExecutionStep {
                            id: step.id,
                            executable: step.executable,
                            args: step.args,
                            cwd_relative: step.cwd_relative,
                            env: step.env,
                            timeout_ms: step.timeout_ms.expect("checked explicit"),
                            continue_on_error: step.continue_on_error,
                        })
                        .collect(),
                    budget: request.execution.budget,
                    execution_profile: request.execution.execution_profile,
                    execution_target: request.execution.execution_target,
                    windows_authority: request.execution.windows_authority,
                    foreign_references: request.execution.foreign_references,
                    host_dependencies: request.execution.host_dependencies,
                },
                wait_ms: request.wait_ms,
                stdout_tail_bytes: request.stdout_tail_bytes,
                stderr_tail_bytes: request.stderr_tail_bytes,
            }));
        }

        Ok(BoundTaskRun::Proposal(TaskRunProposal {
            schema_version: request.schema_version,
            client_request_id: request.client_request_id,
            principal: self.principal.clone(),
            global_limit: self.global_limit,
            execution: ExecutionProposal {
                workspace_id: request.execution.workspace_id,
                executable: first.executable,
                args: first.args,
                cwd_relative: first.cwd_relative,
                env: first.env,
                timeout_ms: request.execution.timeout_ms,
                stdout_limit_bytes: request.execution.stdout_limit_bytes,
                stderr_limit_bytes: request.execution.stderr_limit_bytes,
                steps: request.execution.steps,
                budget: request.execution.budget,
                execution_profile: request.execution.execution_profile,
                execution_target: request.execution.execution_target,
                windows_authority: request.execution.windows_authority,
                foreign_references: request.execution.foreign_references,
                host_dependencies: request.execution.host_dependencies,
            },
            wait_ms: request.wait_ms,
            stdout_tail_bytes: request.stdout_tail_bytes,
            stderr_tail_bytes: request.stderr_tail_bytes,
        }))
    }
}

fn default_patch_diff_bytes() -> u64 {
    MAX_WORKSPACE_IO_BYTES
}

fn default_exec_wait_ms() -> u64 {
    // Public MCP admission should return quickly once durable Job identity exists.
    // Callers that deliberately want a longer synchronous observation may still
    // request any wait up to Core's MAX_TASK_WAIT_MS.
    2_000
}

fn default_exec_tail_bytes() -> u64 {
    4096
}

fn validate_ingress_sha256(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_ingress_relative_object(value: &str) -> bool {
    if value.is_empty() || value.starts_with('/') || value.contains('\\') || value.contains('\0') {
        return false;
    }
    value
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn ingress_ipv4_is_public(value: Ipv4Addr) -> bool {
    let octets = value.octets();
    if value.is_unspecified()
        || value.is_loopback()
        || value.is_private()
        || value.is_link_local()
        || value.is_broadcast()
        || value.is_documentation()
        || value.is_multicast()
    {
        return false;
    }
    if octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || octets[0] >= 240
    {
        return false;
    }
    true
}

fn ingress_ipv6_is_public(value: Ipv6Addr) -> bool {
    if let Some(mapped) = value.to_ipv4_mapped() {
        return ingress_ipv4_is_public(mapped);
    }
    if value.is_unspecified()
        || value.is_loopback()
        || value.is_multicast()
        || value.is_unique_local()
        || value.is_unicast_link_local()
    {
        return false;
    }
    let segments = value.segments();
    !(segments[0] == 0x2001 && segments[1] == 0x0db8)
}

fn ingress_ip_is_public(value: IpAddr) -> bool {
    match value {
        IpAddr::V4(value) => ingress_ipv4_is_public(value),
        IpAddr::V6(value) => ingress_ipv6_is_public(value),
    }
}

fn validate_ingress_download_host_config(host: &str) -> bool {
    if host.is_empty()
        || host != host.to_ascii_lowercase()
        || host.contains('/')
        || host.contains(':')
        || host.contains('@')
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return false;
    }
    match host.parse::<IpAddr>() {
        Ok(ip) => ingress_ip_is_public(ip),
        Err(_) => true,
    }
}

fn ingress_stage_size_digest(
    path: &std::path::Path,
    max_bytes: u64,
) -> Result<(u64, String), ToolError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| ToolError::internal("cannot inspect retained verified input stage"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ToolError::internal(
            "retained verified input stage is not a regular non-symlink file",
        ));
    }
    if metadata.len() > max_bytes {
        return Err(ToolError::invalid(
            "retained input stage exceeds configured maxBytes",
            "file",
        ));
    }
    let mut file = std::fs::File::open(path)
        .map_err(|_| ToolError::internal("cannot open retained verified input stage"))?;
    let mut digest = Sha256::new();
    let mut observed = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|_| ToolError::internal("cannot read retained verified input stage"))?;
        if read == 0 {
            break;
        }
        observed = observed.saturating_add(read as u64);
        if observed > max_bytes {
            return Err(ToolError::invalid(
                "retained input stage exceeds configured maxBytes",
                "file",
            ));
        }
        digest.update(&buffer[..read]);
    }
    Ok((observed, format!("sha256:{:x}", digest.finalize())))
}

async fn ingress_pinned_https_get(
    initial: reqwest::Url,
    allowed_hosts: &[String],
) -> Result<reqwest::Response, ToolError> {
    let mut url = initial;
    for redirect_index in 0..=5 {
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return Err(ToolError::invalid(
                "file.download_url must remain HTTPS without URL userinfo",
                "file.download_url",
            ));
        }
        if url.port_or_known_default() != Some(443) {
            return Err(ToolError::invalid(
                "file.download_url must use the standard HTTPS port",
                "file.download_url",
            ));
        }
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if !allowed_hosts.iter().any(|allowed| allowed == &host) {
            return Err(ToolError::invalid(
                "file.download_url host is not operator-authorized for input ingress",
                "file.download_url",
            ));
        }
        let resolved = tokio::net::lookup_host((host.as_str(), 443))
            .await
            .map_err(|_| ToolError::internal("input file download host resolution failed"))?
            .collect::<Vec<SocketAddr>>();
        if resolved.is_empty()
            || resolved
                .iter()
                .any(|address| !ingress_ip_is_public(address.ip()))
        {
            return Err(ToolError::invalid(
                "file.download_url resolved to a non-public network address",
                "file.download_url",
            ));
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host.as_str(), &resolved)
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .map_err(|_| ToolError::internal("cannot initialize pinned input download client"))?;
        let response = client.get(url.clone()).send().await.map_err(|_| {
            ToolError::internal("input file download failed before a verified local stage existed")
        })?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        if redirect_index == 5 {
            return Err(ToolError::invalid(
                "input file download exceeded the redirect limit",
                "file.download_url",
            ));
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                ToolError::invalid(
                    "input file redirect omitted a valid Location",
                    "file.download_url",
                )
            })?;
        url = url.join(location).map_err(|_| {
            ToolError::invalid(
                "input file redirect Location is invalid",
                "file.download_url",
            )
        })?;
    }
    Err(ToolError::internal(
        "input file redirect loop did not terminate",
    ))
}

#[derive(Clone)]
pub struct ServerConfig {
    pub runtime: RuntimeConfig,
    pub input_authorities: Vec<InputAuthority>,
    pub execution: ExecutionContext,
    pub release: Option<RuntimeReleaseExecutionConfig>,
    pub input_ingress: Option<InputIngressExecutionConfig>,
    pub trace_path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct RuntimeServer {
    state: Arc<ServerState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

struct ServerState {
    runtime: Runtime,
    executor: UniversalExecutorConfig,
    execution: ExecutionContext,
    release: Option<RuntimeReleaseExecutionConfig>,
    input_ingress: Option<InputIngressExecutionConfig>,
    trace_path: Option<PathBuf>,
}

impl RuntimeServer {
    pub fn new(config: ServerConfig) -> Result<Self, ToolError> {
        let executor = config.runtime.executor.clone();
        executor.ensure_store().map_err(ToolError::from)?;
        let runtime = Runtime::new_with_input_authorities(config.runtime, config.input_authorities)
            .map_err(ToolError::from)?;
        if let Some(release) = config.release.as_ref() {
            for (path, field) in [
                (&release.source_repo, "release.sourceRepo"),
                (&release.install_dir, "release.installDir"),
                (&release.database, "release.database"),
                (&release.env_file, "release.envFile"),
                (&release.receipt_root, "release.receiptRoot"),
            ] {
                if !path.is_absolute() {
                    return Err(ToolError::invalid(
                        "Runtime Release paths must be absolute",
                        field,
                    ));
                }
            }
            if release.timeout_ms == 0 || release.timeout_ms > executor.max_runtime_ms {
                return Err(ToolError::invalid(
                    "Runtime Release timeout must fit inside Runtime maxRuntimeMs",
                    "release.timeoutMs",
                ));
            }
        }
        if let Some(ingress) = config.input_ingress.as_ref() {
            if !ingress.staging_root.is_absolute()
                || !ingress.workstation_tool.is_absolute()
                || !ingress.workstation_config.is_absolute()
            {
                return Err(ToolError::invalid(
                    "input ingress paths must be absolute operator-owned paths",
                    "inputIngress",
                ));
            }
            if ingress.authorities.is_empty() {
                return Err(ToolError::invalid(
                    "input ingress must explicitly opt in at least one authority",
                    "inputIngress.authorities",
                ));
            }
            if ingress.download_hosts.is_empty()
                || ingress
                    .download_hosts
                    .iter()
                    .any(|host| !validate_ingress_download_host_config(host))
            {
                return Err(ToolError::invalid(
                    "input ingress must explicitly configure normalized allowed downloadHosts",
                    "inputIngress.downloadHosts",
                ));
            }
            if ingress.max_bytes == 0 || ingress.max_bytes > 512 * 1024 * 1024 {
                return Err(ToolError::invalid(
                    "input ingress maxBytes must be between 1 and 512 MiB",
                    "inputIngress.maxBytes",
                ));
            }
        }
        let state = Arc::new(ServerState {
            runtime,
            executor,
            execution: config.execution,
            release: config.release,
            input_ingress: config.input_ingress,
            trace_path: config.trace_path,
        });
        Ok(Self {
            state,
            tool_router: Self::tool_router(),
        })
    }

    pub fn runtime_handle(&self) -> Runtime {
        self.state.runtime.clone()
    }

    fn decorate_tool_for_host_extensions(&self, mut tool: Tool) -> Tool {
        if tool.name.as_ref() == "input.ingest" && self.state.input_ingress.is_some() {
            tool.meta
                .get_or_insert_default()
                .0
                .insert("openai/fileParams".to_string(), json!(["file"]));
        }
        tool
    }

    pub(crate) fn catalog_tools(&self) -> Vec<Tool> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|tool| self.decorate_tool_for_host_extensions(tool))
            .collect()
    }

    async fn perform_input_ingress(
        &self,
        request: InputIngressToolRequest,
    ) -> Result<InputIngressToolResult, ToolError> {
        if request.schema_version != RUNTIME_SCHEMA_VERSION {
            return Err(ToolError::invalid(
                "schemaVersion must be 1",
                "schemaVersion",
            ));
        }
        let ingress = self.state.input_ingress.clone().ok_or_else(|| {
            ToolError::invalid(
                "input ingress is not configured on this Runtime",
                "authority",
            )
        })?;
        if !ingress
            .authorities
            .iter()
            .any(|value| value == &request.authority)
        {
            return Err(ToolError::invalid(
                "authority is not explicitly enabled for external input ingress",
                "authority",
            ));
        }
        if !validate_ingress_sha256(&request.expected_sha256) {
            return Err(ToolError::invalid(
                "expectedSha256 must be sha256:<64-hex>",
                "expectedSha256",
            ));
        }
        if request.expected_size_bytes == 0 || request.expected_size_bytes > ingress.max_bytes {
            return Err(ToolError::invalid(
                "expectedSizeBytes must be positive and no greater than configured maxBytes",
                "expectedSizeBytes",
            ));
        }
        if !validate_ingress_relative_object(&request.relative_object) {
            return Err(ToolError::invalid(
                "relativeObject must be one normalized POSIX relative path",
                "relativeObject",
            ));
        }
        if request.file.file_id.is_empty() || request.file.file_id.len() > 512 {
            return Err(ToolError::invalid(
                "file.file_id is invalid",
                "file.file_id",
            ));
        }
        let url = reqwest::Url::parse(&request.file.download_url).map_err(|_| {
            ToolError::invalid(
                "file.download_url must be a valid HTTPS URL",
                "file.download_url",
            )
        })?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return Err(ToolError::invalid(
                "file.download_url must use HTTPS without URL userinfo",
                "file.download_url",
            ));
        }
        let initial_host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if !ingress
            .download_hosts
            .iter()
            .any(|host| host == &initial_host)
        {
            return Err(ToolError::invalid(
                "file.download_url host is not operator-authorized for input ingress",
                "file.download_url",
            ));
        }

        std::fs::create_dir_all(&ingress.staging_root).map_err(|_| {
            ToolError::internal("cannot prepare private input-ingress staging root")
        })?;
        let metadata = std::fs::symlink_metadata(&ingress.staging_root).map_err(|_| {
            ToolError::internal("cannot observe private input-ingress staging root")
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ToolError::internal(
                "configured input-ingress staging root is not a real directory",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(
                &ingress.staging_root,
                std::fs::Permissions::from_mode(0o700),
            )
            .map_err(|_| ToolError::internal("cannot make input-ingress staging root private"))?;
        }

        let host_file_reference_digest = format!(
            "sha256:{:x}",
            Sha256::digest(request.file.file_id.as_bytes())
        );
        let expected_sha256 = request.expected_sha256.to_ascii_lowercase();
        let mut identity = Sha256::new();
        identity.update(request.client_request_id.as_bytes());
        identity.update(b"\0");
        identity.update(request.authority.as_bytes());
        identity.update(b"\0");
        identity.update(request.relative_object.as_bytes());
        identity.update(b"\0");
        identity.update(expected_sha256.as_bytes());
        identity.update(b"\0");
        identity.update(request.expected_size_bytes.to_string().as_bytes());
        let key = format!("{:x}", identity.finalize());
        let source_name = format!("{key}.source");
        let request_name = format!("{key}.request.json");
        let source_path = ingress.staging_root.join(&source_name);
        let request_path = ingress.staging_root.join(&request_name);
        let temporary_path = ingress
            .staging_root
            .join(format!(".{key}.{}.part", Uuid::now_v7()));
        let cleanup = |paths: &[&std::path::Path]| {
            for path in paths {
                let _ = std::fs::remove_file(path);
            }
        };
        cleanup(&[&temporary_path]);

        let workstation_request = json!({
            "schemaVersion": 1,
            "requestId": request.client_request_id.clone(),
            "carrier": ingress.workstation_carrier.clone(),
            "sourceObject": source_name,
            "sourceProvenance": {
                "provider": "openai-host-file-param",
                "providerFileId": format!("host-declared-{host_file_reference_digest}"),
            },
            "expectedSha256": expected_sha256.clone(),
            "authority": request.authority.clone(),
            "relativeObject": request.relative_object.clone(),
        });
        let serialized = serde_json::to_vec(&workstation_request)
            .map_err(|_| ToolError::internal("cannot serialize Workstation ingress request"))?;
        if request_path.exists() {
            let retained = std::fs::read(&request_path).map_err(|_| {
                ToolError::internal("cannot read retained private Workstation ingress request")
            })?;
            if retained != serialized {
                return Err(ToolError::invalid(
                    "retained staging request conflicts with same durable input identity",
                    "clientRequestId",
                ));
            }
        } else {
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&request_path)
                .map_err(|_| {
                    ToolError::internal("cannot create private Workstation ingress request")
                })?;
            std::io::Write::write_all(&mut output, &serialized)
                .and_then(|_| output.sync_all())
                .map_err(|_| {
                    ToolError::internal("cannot persist private Workstation ingress request")
                })?;
        }

        let run_workstation = |reconcile_only: bool| -> Result<
            (std::process::ExitStatus, serde_json::Value),
            ToolError,
        > {
            let mut command = std::process::Command::new(&ingress.workstation_tool);
            command
                .arg("--config")
                .arg(&ingress.workstation_config)
                .arg("--request")
                .arg(&request_path);
            if reconcile_only {
                command.arg("--reconcile-only");
            }
            let process = command.output().map_err(|_| {
                ToolError::internal("cannot invoke Workstation input-authority ingress")
            })?;
            let body: serde_json::Value = serde_json::from_slice(&process.stdout)
                .map_err(|_| ToolError::internal("Workstation ingress returned invalid JSON"))?;
            Ok((process.status, body))
        };

        let build_result =
            |receipt: serde_json::Value| -> Result<InputIngressToolResult, ToolError> {
                if receipt.get("byteSize").and_then(serde_json::Value::as_u64)
                    != Some(request.expected_size_bytes)
                {
                    return Err(ToolError::internal(
                        "Workstation receipt byte size differs from expectedSizeBytes",
                    ));
                }
                if receipt
                    .get("observedSha256")
                    .and_then(serde_json::Value::as_str)
                    != Some(expected_sha256.as_str())
                {
                    return Err(ToolError::internal(
                        "Workstation receipt digest differs from expectedSha256",
                    ));
                }
                Ok(InputIngressToolResult {
                    schema_version: 1,
                    kind: "ordivon.runtime.input-ingress-adapter-receipt".to_string(),
                    authority: request.authority.clone(),
                    relative_object: request.relative_object.clone(),
                    expected_sha256: expected_sha256.clone(),
                    expected_size_bytes: request.expected_size_bytes,
                    host_file_reference_digest: host_file_reference_digest.clone(),
                    source_identity_standing: "HOST_DECLARED_UNVERIFIED".to_string(),
                    workstation_receipt: receipt,
                    non_claims: vec![
                        "host file reference digest is not cryptographic provider identity"
                            .to_string(),
                        "byte materialization is not Artifact/domain acceptance".to_string(),
                        "Runtime adapter success is not consumer execution success".to_string(),
                    ],
                })
            };

        let (reconcile_status, reconcile) = run_workstation(true)?;
        if !reconcile_status.success() {
            return Err(ToolError::internal(
                "Workstation ingress reconciliation failed closed",
            ));
        }
        if reconcile
            .get("commitStanding")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.starts_with("COMMITTED"))
        {
            let result = build_result(reconcile)?;
            cleanup(&[&source_path, &request_path]);
            return Ok(result);
        }
        if reconcile
            .get("standing")
            .and_then(serde_json::Value::as_str)
            != Some("SOURCE_REQUIRED")
        {
            return Err(ToolError::internal(
                "Workstation ingress reconciliation did not admit a source fetch",
            ));
        }

        let staged_valid = if source_path.exists() {
            match ingress_stage_size_digest(&source_path, ingress.max_bytes) {
                Ok((size, digest))
                    if size == request.expected_size_bytes && digest == expected_sha256 =>
                {
                    true
                }
                _ => {
                    cleanup(&[&source_path]);
                    false
                }
            }
        } else {
            false
        };

        if !staged_valid {
            let mut response = ingress_pinned_https_get(url, &ingress.download_hosts).await?;
            if !response.status().is_success() {
                return Err(ToolError::internal(format!(
                    "input file download returned HTTP {}",
                    response.status().as_u16()
                )));
            }
            if let Some(length) = response.content_length() {
                if length != request.expected_size_bytes {
                    return Err(ToolError::invalid(
                        "input file Content-Length differs from expectedSizeBytes",
                        "expectedSizeBytes",
                    ));
                }
            }
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .map_err(|_| ToolError::internal("cannot create private input staging file"))?;
            let mut digest = Sha256::new();
            let mut observed_bytes = 0_u64;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| ToolError::internal("input file download stream failed"))?
            {
                observed_bytes = observed_bytes.saturating_add(chunk.len() as u64);
                if observed_bytes > request.expected_size_bytes {
                    cleanup(&[&temporary_path]);
                    return Err(ToolError::invalid(
                        "input file stream exceeds expectedSizeBytes",
                        "expectedSizeBytes",
                    ));
                }
                digest.update(&chunk);
                std::io::Write::write_all(&mut output, &chunk)
                    .map_err(|_| ToolError::internal("cannot write private input staging bytes"))?;
            }
            std::io::Write::flush(&mut output)
                .and_then(|_| output.sync_all())
                .map_err(|_| ToolError::internal("cannot fsync private input staging bytes"))?;
            drop(output);
            if observed_bytes != request.expected_size_bytes {
                cleanup(&[&temporary_path]);
                return Err(ToolError::invalid(
                    "input file stream length differs from expectedSizeBytes",
                    "expectedSizeBytes",
                ));
            }
            let observed_digest = format!("sha256:{:x}", digest.finalize());
            if observed_digest != expected_sha256 {
                cleanup(&[&temporary_path]);
                return Err(ToolError::invalid(
                    "downloaded input bytes do not match expectedSha256",
                    "expectedSha256",
                ));
            }
            match std::fs::hard_link(&temporary_path, &source_path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let (size, digest) =
                        ingress_stage_size_digest(&source_path, ingress.max_bytes)?;
                    if size != request.expected_size_bytes || digest != expected_sha256 {
                        cleanup(&[&temporary_path]);
                        return Err(ToolError::internal(
                            "concurrent input stage conflicts with expected bytes",
                        ));
                    }
                }
                Err(_) => {
                    cleanup(&[&temporary_path]);
                    return Err(ToolError::internal(
                        "cannot publish verified private input stage",
                    ));
                }
            }
            cleanup(&[&temporary_path]);
        }

        let (status, receipt) = run_workstation(false)?;
        let receipt = if status.success()
            && receipt
                .get("commitStanding")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.starts_with("COMMITTED"))
        {
            receipt
        } else {
            let (retry_status, recovered) = run_workstation(true)?;
            if !retry_status.success()
                || !recovered
                    .get("commitStanding")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| value.starts_with("COMMITTED"))
            {
                return Err(ToolError::internal(
                    "Workstation input-authority ingress did not converge to a committed receipt",
                ));
            }
            recovered
        };
        let result = build_result(receipt)?;
        cleanup(&[&source_path, &request_path]);
        Ok(result)
    }

    pub fn tool_catalog_digest(&self) -> String {
        let mut tools = self.catalog_tools();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let bytes = serde_json::to_vec(&tools)
            .expect("Tool catalog serialization is infallible for generated schemas");
        format!("sha256:{:x}", Sha256::digest(bytes))
    }

    pub(crate) fn discovery_result(&self) -> DiscoverResult {
        let mut result = DiscoverResult::from_server_info(
            self.supported_protocol_versions().into_owned(),
            self.get_info(),
        );
        result.meta.get_or_insert_default().0.insert(
            "com.ordivon/runtime/toolCatalogDigest".to_string(),
            serde_json::Value::String(self.tool_catalog_digest()),
        );
        result
    }
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TraceSummary {
    pub trace_id: String,
    pub core_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorOrigin {
    McpAdapter,
    RuntimeCore,
    WorkspaceExecutor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolRetryClass {
    Never,
    SafeSameRequest,
    ReconcileFirst,
    /// Workspace-scope capacity rejection: the holder Job is identified in
    /// `capacity.holderJobIds`. Observe the holder to terminal, re-observe the
    /// Workspace, reassess the original intent, then submit a fresh request —
    /// never blindly resubmit, because the Workspace state basis may have moved.
    ObserveThenReassess,
    /// Global-scope capacity rejection: another Workspace holds the capacity.
    /// Wait for capacity (observe a holder or retryAfterMs backoff), then the
    /// same request may be retried unchanged.
    WaitThenRetry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolCommitState {
    NotStarted,
    NotCommitted,
    /// A durable Runtime operation identity is known to exist; reconcile it instead of creating new work.
    Committed,
    Unknown,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolError {
    pub code: String,
    pub message: String,
    #[serde(flatten)]
    context: Box<ToolErrorContext>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolErrorContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub origin: ToolErrorOrigin,
    pub retry_class: ToolRetryClass,
    pub commit_state: ToolCommitState,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<Box<RuntimeCapacity>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}

impl std::ops::Deref for ToolError {
    type Target = ToolErrorContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl std::ops::DerefMut for ToolError {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.context
    }
}

impl ToolError {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "INTERNAL_ERROR".to_string(),
            message: message.into(),
            context: Box::new(ToolErrorContext {
                field: None,
                origin: ToolErrorOrigin::McpAdapter,
                retry_class: ToolRetryClass::SafeSameRequest,
                commit_state: ToolCommitState::NotStarted,
                retryable: true,
                retry_after_ms: None,
                capacity: None,
                trace_id: None,
                operation_id: None,
            }),
        }
    }

    fn invalid(message: impl Into<String>, field: &str) -> Self {
        Self {
            code: "INVALID_REQUEST".to_string(),
            message: message.into(),
            context: Box::new(ToolErrorContext {
                field: Some(field.to_string()),
                origin: ToolErrorOrigin::McpAdapter,
                retry_class: ToolRetryClass::Never,
                commit_state: ToolCommitState::NotStarted,
                retryable: false,
                retry_after_ms: None,
                capacity: None,
                trace_id: None,
                operation_id: None,
            }),
        }
    }
}

impl From<RuntimeError> for ToolError {
    fn from(error: RuntimeError) -> Self {
        let code = serde_json::to_value(&error.code)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "EXECUTION_ERROR".to_string());
        let committed_operation = error.operation_id.is_some();
        let capacity_scope = error
            .capacity
            .as_deref()
            .map(|capacity| capacity.scope.clone());
        let (retry_class, commit_state) = if committed_operation {
            (ToolRetryClass::ReconcileFirst, ToolCommitState::Committed)
        } else {
            match error.code {
                ordivon_runtime_core::RuntimeErrorCode::DispatchOutcomeUnknown
                | ordivon_runtime_core::RuntimeErrorCode::ReconciliationRequired => {
                    (ToolRetryClass::ReconcileFirst, ToolCommitState::Unknown)
                }
                ordivon_runtime_core::RuntimeErrorCode::WorkspaceExists => {
                    (ToolRetryClass::ReconcileFirst, ToolCommitState::NotStarted)
                }
                ordivon_runtime_core::RuntimeErrorCode::ConcurrencyLimit => {
                    match capacity_scope.as_deref() {
                        // The target Workspace itself holds the single-writer slot.
                        // The Agent must observe the holder Job, re-observe the
                        // Workspace, reassess the original intent, then submit a
                        // fresh request: the state basis may have moved by then.
                        Some("workspace") => (
                            ToolRetryClass::ObserveThenReassess,
                            ToolCommitState::NotStarted,
                        ),
                        // Another Workspace consumed the global capacity pool.
                        // The target Workspace has no active writer; the same
                        // request may be retried after capacity frees.
                        Some("global") => {
                            (ToolRetryClass::WaitThenRetry, ToolCommitState::NotStarted)
                        }
                        _ => (ToolRetryClass::SafeSameRequest, ToolCommitState::NotStarted),
                    }
                }
                ordivon_runtime_core::RuntimeErrorCode::DeploymentInProgress
                | ordivon_runtime_core::RuntimeErrorCode::RegistryBusy
                | ordivon_runtime_core::RuntimeErrorCode::WorkspaceBusy => {
                    (ToolRetryClass::SafeSameRequest, ToolCommitState::NotStarted)
                }
                _ if error.retryable => (
                    ToolRetryClass::SafeSameRequest,
                    ToolCommitState::NotCommitted,
                ),
                _ => (ToolRetryClass::Never, ToolCommitState::NotCommitted),
            }
        };
        Self {
            code,
            message: error.message,
            context: Box::new(ToolErrorContext {
                field: error.field,
                origin: ToolErrorOrigin::RuntimeCore,
                retry_class,
                commit_state,
                retryable: if committed_operation {
                    false
                } else {
                    error.retryable
                },
                retry_after_ms: error.retry_after_ms,
                capacity: error.capacity,
                trace_id: None,
                operation_id: error.operation_id,
            }),
        }
    }
}

impl From<UniversalExecError> for ToolError {
    fn from(error: UniversalExecError) -> Self {
        let code = serde_json::to_value(&error.code)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "UNIVERSAL_EXEC_ERROR".to_string());
        let mutation_outcome_unknown = matches!(
            error.code,
            ordivon_runtime_core::UniversalExecErrorCode::WorkspaceMutationIncomplete
        );
        Self {
            code,
            message: error.message,
            context: Box::new(ToolErrorContext {
                field: error.field,
                origin: ToolErrorOrigin::WorkspaceExecutor,
                retry_class: if mutation_outcome_unknown {
                    ToolRetryClass::ReconcileFirst
                } else if error.retryable {
                    ToolRetryClass::SafeSameRequest
                } else {
                    ToolRetryClass::Never
                },
                commit_state: if mutation_outcome_unknown {
                    ToolCommitState::Unknown
                } else {
                    ToolCommitState::NotCommitted
                },
                retryable: error.retryable,
                retry_after_ms: None,
                capacity: None,
                trace_id: None,
                operation_id: None,
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolErrorEnvelope {
    pub error: ToolError,
}

#[derive(Clone, Debug)]
pub enum ToolOutcome<T> {
    Success(T),
    Error(ToolError),
}

impl<T: JsonSchema> JsonSchema for ToolOutcome<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Owned(format!("ToolOutcome_for_{}", T::schema_name()))
    }

    fn schema_id() -> Cow<'static, str> {
        Cow::Owned(format!("ordivon::ToolOutcome<{}>", T::schema_id()))
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let success = generator.subschema_for::<T>();
        let error = generator.subschema_for::<ToolErrorEnvelope>();
        schemars::json_schema!({
            "oneOf": [success, error]
        })
    }
}

impl<T> IntoCallToolResult for ToolOutcome<T>
where
    T: Serialize + JsonSchema + Send + 'static,
{
    fn into_call_tool_result(self) -> Result<CallToolResponse, McpError> {
        let (ok, value, compatibility_text) = match self {
            Self::Success(result) => {
                let value = serde_json::to_value(result).map_err(|error| {
                    McpError::internal_error(format!("cannot serialize tool result: {error}"), None)
                })?;
                (true, value, "ok".to_string())
            }
            Self::Error(error) => {
                let compatibility_text = error.message.clone();
                (false, json!({ "error": error }), compatibility_text)
            }
        };
        let mut result = if ok {
            CallToolResult::success(Vec::new())
        } else {
            CallToolResult::error(vec![ContentBlock::text(compatibility_text)])
        };
        result.structured_content = Some(value);
        Ok(result.into())
    }
}

fn workspace_content_call_result(
    outcome: ToolOutcome<ordivon_runtime_core::WorkspaceContentReadResult>,
) -> Result<CallToolResult, McpError> {
    match outcome {
        ToolOutcome::Success(result) => {
            let structured = serde_json::to_value(&result.metadata).map_err(|error| {
                McpError::internal_error(
                    format!("cannot serialize workspace content metadata: {error}"),
                    None,
                )
            })?;
            let block = ContentBlock::image(
                BASE64_STANDARD.encode(&result.bytes),
                result.metadata.media_type.clone(),
            );
            let mut response = CallToolResult::success(vec![block]);
            response.structured_content = Some(structured);
            Ok(response)
        }
        ToolOutcome::Error(error) => {
            let message = error.message.clone();
            let structured = json!({ "error": error });
            let mut response = CallToolResult::error(vec![ContentBlock::text(message)]);
            response.structured_content = Some(structured);
            Ok(response)
        }
    }
}

impl RuntimeServer {
    async fn run_core<T, F>(&self, tool: &'static str, operation: F) -> ToolOutcome<T>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, ToolError> + Send + 'static,
    {
        let trace_id = next_trace_id("core");
        let total_started = Instant::now();
        let core_started = Instant::now();
        let joined = tokio::task::spawn_blocking(operation).await;
        let core_ms = elapsed_ms(core_started);
        let result = match joined {
            Ok(result) => result,
            Err(error) => Err(ToolError::internal(format!(
                "blocking operation failed to join: {error}"
            ))),
        };
        let trace = TraceSummary {
            trace_id,
            core_ms,
            total_ms: elapsed_ms(total_started),
        };
        self.record_trace(tool, &trace, result.is_ok());
        match result {
            Ok(value) => ToolOutcome::Success(value),
            Err(mut error) => {
                error.trace_id = Some(trace.trace_id);
                ToolOutcome::Error(error)
            }
        }
    }

    fn record_trace(&self, tool: &str, trace: &TraceSummary, ok: bool) {
        let Some(path) = &self.state.trace_path else {
            return;
        };
        let _guard = match GLOBAL_TRACE_LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!("trace lock poisoned: {error}");
                return;
            }
        };
        let record = json!({
            "traceId": trace.trace_id,
            "tool": tool,
            "ok": ok,
            "coreMs": trace.core_ms,
            "totalMs": trace.total_ms,
            "observedUnixMs": unix_ms(),
        });
        let write_result = append_rotating_jsonl(path, &record, DEFAULT_TRACE_ROTATION_BYTES);
        if let Err(error) = write_result {
            tracing::warn!("cannot append trace {}: {error}", path.display());
        }
    }
}

fn next_trace_id(kind: &str) -> String {
    format!(
        "ordivon-{kind}-{}-{}-{}",
        std::process::id(),
        unix_ms(),
        GLOBAL_TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

mod handler;
mod tools;

#[cfg(test)]
mod tests;
