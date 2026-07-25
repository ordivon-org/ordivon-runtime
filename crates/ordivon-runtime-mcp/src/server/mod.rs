use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ordivon_runtime_core::{
    read_workspace_slice_compact, read_workspace_text_compact, workspace_diff_compact,
    ArtifactReadRequest, ArtifactReadResult, CompactWorkspaceDiffResult,
    CompactWorkspaceOpenResult, GitWorkspaceCreateRequest, Runtime, RuntimeCapacity, RuntimeConfig,
    RuntimeError, RuntimeJobListRequest, RuntimeJobListResult, TaskCancelRequest, TaskObservation,
    TaskObserveRequest, TaskRunRequest, UniversalExecError, UniversalExecutionRequest,
    UniversalExecutorConfig, WorkspaceCloseRequest, WorkspaceCloseResult,
    WorkspaceDiffRequest as ExecWorkspaceDiffRequest, WorkspaceMutateRequest,
    WorkspaceMutateResult, WorkspaceReadRequest as ExecWorkspaceReadRequest,
    WorkspaceReadSliceRequest, MAX_TASK_TAIL_BYTES, MAX_TASK_WAIT_MS, MAX_WORKSPACE_IO_BYTES,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::IntoCallToolResult;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

static GLOBAL_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static GLOBAL_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<Box<RuntimeCapacity>>,
}

impl ToolError {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "INTERNAL_ERROR".to_string(),
            message: message.into(),
            field: None,
            retryable: true,
            retry_after_ms: None,
            capacity: None,
        }
    }

    fn invalid(message: impl Into<String>, field: &str) -> Self {
        Self {
            code: "INVALID_REQUEST".to_string(),
            message: message.into(),
            field: Some(field.to_string()),
            retryable: false,
            retry_after_ms: None,
            capacity: None,
        }
    }
}

impl From<RuntimeError> for ToolError {
    fn from(error: RuntimeError) -> Self {
        let code = serde_json::to_value(&error.code)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "EXECUTION_ERROR".to_string());
        Self {
            code,
            message: error.message,
            field: error.field,
            retryable: error.retryable,
            retry_after_ms: error.retry_after_ms,
            capacity: error.capacity,
        }
    }
}

impl From<UniversalExecError> for ToolError {
    fn from(error: UniversalExecError) -> Self {
        let code = serde_json::to_value(&error.code)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "UNIVERSAL_EXEC_ERROR".to_string());
        Self {
            code,
            message: error.message,
            field: error.field,
            retryable: error.retryable,
            retry_after_ms: None,
            capacity: None,
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
    fn into_call_tool_result(self) -> Result<CallToolResult, McpError> {
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
        Ok(result)
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
            Err(error) => ToolOutcome::Error(error),
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
        let write_result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| {
                serde_json::to_writer(&mut file, &record)?;
                file.write_all(b"\n")
            });
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

fn parse_workspace_exec(arguments: Option<JsonObject>) -> Result<WorkspaceExecRequest, McpError> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(|error| {
        McpError::invalid_params(
            format!("invalid workspace.exec task arguments: {error}"),
            None,
        )
    })
}

fn mcp_error(error: ToolError) -> McpError {
    let data = serde_json::to_value(&error).ok();
    if matches!(
        error.code.as_str(),
        "INVALID_REQUEST" | "JOB_NOT_FOUND" | "ATTEMPT_NOT_FOUND" | "IDEMPOTENCY_CONFLICT"
    ) {
        McpError::invalid_params(error.message, data)
    } else {
        McpError::internal_error(error.message, data)
    }
}

fn format_unix_ms(value: u64) -> Result<String, McpError> {
    let nanos = i128::from(value) * 1_000_000;
    let timestamp = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|error| McpError::internal_error(format!("invalid task time: {error}"), None))?;
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| {
            McpError::internal_error(format!("cannot format task time: {error}"), None)
        })
}

fn encode_cursor(cursor: &ordivon_runtime_core::RuntimeJobListCursor) -> String {
    format!("{}:{}", cursor.created_at_ms, cursor.job_id)
}

fn decode_cursor(
    value: Option<String>,
) -> Result<Option<ordivon_runtime_core::RuntimeJobListCursor>, McpError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let (created, job_id) = value
        .split_once(':')
        .ok_or_else(|| McpError::invalid_params("invalid task cursor", None))?;
    let created_at_ms = created
        .parse()
        .map_err(|_| McpError::invalid_params("invalid task cursor timestamp", None))?;
    Ok(Some(ordivon_runtime_core::RuntimeJobListCursor {
        created_at_ms,
        job_id: job_id.to_string(),
    }))
}

mod tasks;
mod tools;

#[cfg(test)]
mod tests;
