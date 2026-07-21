use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::m4::{
    M4ReadMode, M4WorkspaceDiffRequest, M4WorkspaceReadRequest, M4WorkspaceReadResult,
    M5DogfoodPolicy,
};
use ordivon_exec::{
    create_git_workspace_compact, mutate_workspace, read_workspace_slice_compact,
    read_workspace_text_compact, workspace_diff_compact, CompactWorkspaceDiffResult,
    CompactWorkspaceOpenResult, GitWorkspaceCreateRequest, JobListRequestM6, JobListResultM6,
    M6ArtifactReadRequest, M6ArtifactReadResult, M6Error, M6Runtime, M6RuntimeConfig,
    M6TaskCancelRequest, M6TaskObservation, M6TaskObserveRequest, M6TaskRunRequest,
    UniversalExecError, UniversalExecutorConfig, WorkspaceDiffRequest, WorkspaceMutateRequest,
    WorkspaceMutateResult, WorkspaceReadRequest, WorkspaceReadSliceRequest,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::IntoCallToolResult;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde_json::{json, Value};

static GLOBAL_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static GLOBAL_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone)]
pub struct M6ServerConfig {
    pub runtime: M6RuntimeConfig,
    pub trace_path: Option<PathBuf>,
    pub dogfood_policy: Option<M5DogfoodPolicy>,
}

#[derive(Clone)]
pub struct M6Server {
    state: Arc<M6State>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

struct M6State {
    runtime: M6Runtime,
    executor: UniversalExecutorConfig,
    trace_path: Option<PathBuf>,
    dogfood_policy: Option<M5DogfoodPolicy>,
}

impl M6Server {
    pub fn new(mut config: M6ServerConfig) -> Result<Self, M6McpError> {
        let executor = config.runtime.executor.clone();
        executor.ensure_store().map_err(M6McpError::from)?;
        config.dogfood_policy = config
            .dogfood_policy
            .take()
            .map(M5DogfoodPolicy::canonicalized)
            .transpose()
            .map_err(|error| M6McpError {
                code: error.code,
                message: error.message,
                field: error.field,
                retryable: error.retryable,
            })?;
        let runtime = M6Runtime::new(config.runtime).map_err(M6McpError::from)?;
        let state = Arc::new(M6State {
            runtime,
            executor,
            trace_path: config.trace_path,
            dogfood_policy: config.dogfood_policy,
        });
        Ok(Self {
            state,
            tool_router: Self::tool_router(),
        })
    }
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct M6TraceSummary {
    pub trace_id: String,
    pub core_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct M6McpError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
}

impl M6McpError {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "M6_INTERNAL_ERROR".to_string(),
            message: message.into(),
            field: None,
            retryable: true,
        }
    }

    fn invalid(message: impl Into<String>, field: &str) -> Self {
        Self {
            code: "INVALID_REQUEST".to_string(),
            message: message.into(),
            field: Some(field.to_string()),
            retryable: false,
        }
    }
}

impl From<M6Error> for M6McpError {
    fn from(error: M6Error) -> Self {
        let code = serde_json::to_value(&error.code)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "M6_EXEC_ERROR".to_string());
        Self {
            code,
            message: error.message,
            field: error.field,
            retryable: error.retryable,
        }
    }
}

impl From<UniversalExecError> for M6McpError {
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
        }
    }
}

#[derive(Clone, Debug)]
pub enum M6Outcome<T> {
    Success(T),
    Error(M6McpError),
}

impl<T: JsonSchema> JsonSchema for M6Outcome<T> {
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

impl<T> IntoCallToolResult for M6Outcome<T>
where
    T: Serialize + JsonSchema + Send + 'static,
{
    fn into_call_tool_result(self) -> Result<CallToolResult, McpError> {
        let (ok, value, compatibility_text) = match self {
            Self::Success(result) => {
                let value = serde_json::to_value(result).map_err(|error| {
                    McpError::internal_error(format!("cannot serialize M6 result: {error}"), None)
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

impl M6Server {
    async fn run_core<T, F>(&self, tool: &'static str, operation: F) -> M6Outcome<T>
    where
        T: Serialize + JsonSchema + Send + 'static,
        F: FnOnce() -> Result<T, M6McpError> + Send + 'static,
    {
        let trace_id = next_trace_id("core");
        let total_started = Instant::now();
        let core_started = Instant::now();
        let joined = tokio::task::spawn_blocking(operation).await;
        let core_ms = elapsed_ms(core_started);
        let result = match joined {
            Ok(result) => result,
            Err(error) => Err(M6McpError::internal(format!(
                "M6 blocking operation failed to join: {error}"
            ))),
        };
        let trace = M6TraceSummary {
            trace_id,
            core_ms,
            total_ms: elapsed_ms(total_started),
        };
        self.record_trace(tool, &trace, result.is_ok());
        match result {
            Ok(value) => M6Outcome::Success(value),
            Err(error) => M6Outcome::Error(error),
        }
    }

    fn record_trace(&self, tool: &str, trace: &M6TraceSummary, ok: bool) {
        let Some(path) = &self.state.trace_path else {
            return;
        };
        let _guard = match GLOBAL_TRACE_LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!("M6 trace lock poisoned: {error}");
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
            tracing::warn!("cannot append M6 trace {}: {error}", path.display());
        }
    }
}

fn next_trace_id(kind: &str) -> String {
    format!(
        "m6-{kind}-{}-{}-{}",
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

fn parse_task_run(arguments: Option<JsonObject>) -> Result<M6TaskRunRequest, McpError> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(|error| {
        McpError::invalid_params(
            format!("invalid M6 workspace.exec task arguments: {error}"),
            None,
        )
    })
}

fn mcp_error_from_m6(error: M6McpError) -> McpError {
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

fn encode_cursor(cursor: &ordivon_exec::JobListCursorM6) -> String {
    format!("{}:{}", cursor.created_at_ms, cursor.job_id)
}

fn decode_cursor(value: Option<String>) -> Result<Option<ordivon_exec::JobListCursorM6>, McpError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let (created, job_id) = value
        .split_once(':')
        .ok_or_else(|| McpError::invalid_params("invalid M6 task cursor", None))?;
    let created_at_ms = created
        .parse()
        .map_err(|_| McpError::invalid_params("invalid M6 task cursor timestamp", None))?;
    Ok(Some(ordivon_exec::JobListCursorM6 {
        created_at_ms,
        job_id: job_id.to_string(),
    }))
}

mod tasks;
mod tools;

#[cfg(test)]
mod tests;
