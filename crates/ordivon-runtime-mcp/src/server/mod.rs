use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ordivon_runtime_core::{
    read_workspace_slice_compact, read_workspace_text_compact, workspace_diff_compact,
    ArtifactReadRequest, ArtifactReadResult, CompactWorkspaceDiffResult,
    CompactWorkspaceOpenResult, ExecutionBudget, ExecutionProfile, ForeignReference,
    GitWorkspaceCreateRequest, Runtime, RuntimeCapacity, RuntimeConfig, RuntimeError,
    RuntimeJobListRequest, RuntimeJobListResult, RuntimeWorkspaceGetRequest,
    RuntimeWorkspaceListRequest, RuntimeWorkspaceListResult, RuntimeWorkspaceSummary,
    TaskCancelRequest, TaskObservation, TaskObserveRequest, TaskRunRequest, UniversalExecError,
    UniversalExecutionRequest, UniversalExecutionStep, UniversalExecutorConfig,
    WorkspaceCloseRequest, WorkspaceCloseResult, WorkspaceDiffRequest as ExecWorkspaceDiffRequest,
    WorkspaceMutateRequest, WorkspaceMutateResult,
    WorkspaceReadRequest as ExecWorkspaceReadRequest, WorkspaceReadSliceRequest,
    MAX_TASK_TAIL_BYTES, MAX_TASK_WAIT_MS, MAX_WORKSPACE_IO_BYTES,
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

use crate::{append_rotating_jsonl, DEFAULT_TRACE_ROTATION_BYTES};

static GLOBAL_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static GLOBAL_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceOpenRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    pub schema_version: u32,
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
    pub schema_version: u32,
    pub workspace_id: String,
    #[schemars(range(min = 1, max = MAX_WORKSPACE_IO_BYTES))]
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub client_request_id: String,
    pub execution: UniversalExecutionRequest,
    #[serde(default = "default_exec_wait_ms")]
    #[schemars(range(max = MAX_TASK_WAIT_MS))]
    pub wait_ms: u64,
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stdout_tail_bytes: u64,
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stderr_tail_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecPlanInput {
    pub workspace_id: String,
    #[schemars(length(min = 1, max = 32))]
    pub steps: Vec<UniversalExecutionStep>,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
    #[serde(default, skip_serializing_if = "ExecutionBudget::is_empty")]
    pub budget: ExecutionBudget,
    #[serde(default)]
    pub execution_profile: ExecutionProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_references: Vec<ForeignReference>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceExecPlanRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub client_request_id: String,
    pub execution: WorkspaceExecPlanInput,
    #[serde(default = "default_exec_wait_ms")]
    #[schemars(range(max = MAX_TASK_WAIT_MS))]
    pub wait_ms: u64,
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stdout_tail_bytes: u64,
    #[serde(default = "default_exec_tail_bytes")]
    #[schemars(range(max = MAX_TASK_TAIL_BYTES))]
    pub stderr_tail_bytes: u64,
}

#[derive(Clone)]
pub struct ExecutionContext {
    pub principal: String,
    pub global_limit: u32,
}

impl ExecutionContext {
    fn bind(&self, request: WorkspaceExecRequest) -> TaskRunRequest {
        TaskRunRequest {
            schema_version: request.schema_version,
            client_request_id: request.client_request_id,
            principal: self.principal.clone(),
            global_limit: self.global_limit,
            execution: request.execution,
            wait_ms: request.wait_ms,
            stdout_tail_bytes: request.stdout_tail_bytes,
            stderr_tail_bytes: request.stderr_tail_bytes,
        }
    }

    fn bind_plan(&self, request: WorkspaceExecPlanRequest) -> Result<TaskRunRequest, ToolError> {
        let first = request.execution.steps.first().ok_or_else(|| {
            ToolError::invalid("steps must contain at least one item", "execution.steps")
        })?;
        let timeout_ms = request
            .execution
            .steps
            .iter()
            .try_fold(0_u64, |total, step| total.checked_add(step.timeout_ms))
            .ok_or_else(|| ToolError::invalid("step timeout sum overflowed", "execution.steps"))?;
        Ok(TaskRunRequest {
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
                stdout_limit_bytes: request.execution.stdout_limit_bytes,
                stderr_limit_bytes: request.execution.stderr_limit_bytes,
                steps: request.execution.steps,
                budget: request.execution.budget,
                execution_profile: request.execution.execution_profile,
                foreign_references: request.execution.foreign_references,
            },
            wait_ms: request.wait_ms,
            stdout_tail_bytes: request.stdout_tail_bytes,
            stderr_tail_bytes: request.stderr_tail_bytes,
        })
    }
}

fn default_exec_wait_ms() -> u64 {
    30_000
}

fn default_exec_tail_bytes() -> u64 {
    4096
}

#[derive(Clone)]
pub struct ServerConfig {
    pub runtime: RuntimeConfig,
    pub execution: ExecutionContext,
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
    trace_path: Option<PathBuf>,
}

impl RuntimeServer {
    pub fn new(config: ServerConfig) -> Result<Self, ToolError> {
        let executor = config.runtime.executor.clone();
        executor.ensure_store().map_err(ToolError::from)?;
        let runtime = Runtime::new(config.runtime).map_err(ToolError::from)?;
        let state = Arc::new(ServerState {
            runtime,
            executor,
            execution: config.execution,
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

    pub fn tool_catalog_digest(&self) -> String {
        let mut tools = self.tool_router.list_all();
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
    pub origin: String,
    pub retry_class: String,
    pub commit_state: String,
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
                origin: "mcp_adapter".to_string(),
                retry_class: "safe_same_request".to_string(),
                commit_state: "not_started".to_string(),
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
                origin: "mcp_adapter".to_string(),
                retry_class: "never".to_string(),
                commit_state: "not_started".to_string(),
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
        let (retry_class, commit_state) = match error.code {
            ordivon_runtime_core::RuntimeErrorCode::DispatchOutcomeUnknown
            | ordivon_runtime_core::RuntimeErrorCode::ReconciliationRequired => {
                ("reconcile_first", "unknown")
            }
            ordivon_runtime_core::RuntimeErrorCode::ConcurrencyLimit
            | ordivon_runtime_core::RuntimeErrorCode::RegistryBusy
            | ordivon_runtime_core::RuntimeErrorCode::WorkspaceBusy => {
                ("safe_same_request", "not_started")
            }
            _ if error.retryable => ("safe_same_request", "not_committed"),
            _ => ("never", "not_committed"),
        };
        Self {
            code,
            message: error.message,
            context: Box::new(ToolErrorContext {
                field: error.field,
                origin: "runtime_core".to_string(),
                retry_class: retry_class.to_string(),
                commit_state: commit_state.to_string(),
                retryable: error.retryable,
                retry_after_ms: error.retry_after_ms,
                capacity: error.capacity,
                trace_id: None,
                operation_id: None,
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
                origin: "workspace_executor".to_string(),
                retry_class: if mutation_outcome_unknown {
                    "reconcile_first".to_string()
                } else if error.retryable {
                    "safe_same_request".to_string()
                } else {
                    "never".to_string()
                },
                commit_state: if mutation_outcome_unknown {
                    "unknown".to_string()
                } else {
                    "not_committed".to_string()
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

#[derive(Clone, Debug)]
pub enum ToolOutcome<T> {
    Success(T),
    Error(ToolError),
}

impl<T: JsonSchema> JsonSchema for ToolOutcome<T> {
    fn inline_schema() -> bool {
        T::inline_schema()
    }

    fn schema_name() -> Cow<'static, str> {
        T::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        T::schema_id()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
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

impl RuntimeServer {
    async fn run_core<T, F>(&self, tool: &'static str, operation: F) -> ToolOutcome<T>
    where
        T: Serialize + JsonSchema + Send + 'static,
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
