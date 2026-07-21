use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ordivon_exec::{
    await_universal_task_compact, cancel_universal_task, create_git_workspace_compact,
    mutate_workspace, read_task_artifact, read_workspace_slice_compact,
    read_workspace_text_compact, run_universal_task_compact, snapshot_universal_task,
    start_universal_task, workspace_diff_compact, ArtifactReadRequest, ArtifactReadResult,
    CompactTaskObservation, CompactWorkspaceDiffResult, CompactWorkspaceOpenResult,
    DurableTaskSnapshot, GitWorkspaceCreateRequest, MigrationTaskHandle, MigrationTaskStatus,
    TaskAwaitRequest, TaskCancelRequest, TaskRunRequest, UniversalExecError,
    UniversalExecutorConfig, WorkspaceDiffRequest, WorkspaceMutateRequest, WorkspaceMutateResult,
    WorkspaceReadRequest, WorkspaceReadSliceRequest,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::IntoCallToolResult;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
static GLOBAL_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct M4ServerConfig {
    pub executor: UniversalExecutorConfig,
    pub trace_path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct M4Server {
    state: Arc<M4State>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

struct M4State {
    config: M4ServerConfig,
    trace_lock: Mutex<()>,
}

impl M4Server {
    pub fn new(config: M4ServerConfig) -> Result<Self, M4Error> {
        config.executor.ensure_store().map_err(M4Error::from)?;
        fs::create_dir_all(native_projection_root(&config.executor)).map_err(|error| {
            M4Error::internal(format!("cannot create M4 projection store: {error}"))
        })?;
        let state = Arc::new(M4State {
            config,
            trace_lock: Mutex::new(()),
        });
        Ok(Self {
            state,
            tool_router: Self::tool_router(),
        })
    }
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct M4TraceSummary {
    pub trace_id: String,
    pub core_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct M4Error {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
}

impl M4Error {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "M4_INTERNAL_ERROR".to_string(),
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
impl From<UniversalExecError> for M4Error {
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

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct M4Outcome<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<M4Error>,
    pub trace: M4TraceSummary,
}

impl<T> IntoCallToolResult for M4Outcome<T>
where
    T: Serialize + JsonSchema + Send + 'static,
{
    fn into_call_tool_result(self) -> Result<CallToolResult, McpError> {
        let ok = self.ok;
        let value = serde_json::to_value(self).map_err(|error| {
            McpError::internal_error(format!("cannot serialize M4 result: {error}"), None)
        })?;
        Ok(if ok {
            CallToolResult::structured(value)
        } else {
            CallToolResult::structured_error(value)
        })
    }
}
impl M4Server {
    async fn run_core<T, F>(&self, tool: &'static str, operation: F) -> M4Outcome<T>
    where
        T: Serialize + JsonSchema + Send + 'static,
        F: FnOnce() -> Result<T, M4Error> + Send + 'static,
    {
        let trace_id = format!(
            "m4-{}-{}-{}",
            std::process::id(),
            unix_ms(),
            GLOBAL_TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let total_started = Instant::now();
        let core_started = Instant::now();
        let joined = tokio::task::spawn_blocking(operation).await;
        let core_ms = elapsed_ms(core_started);
        let result = match joined {
            Ok(result) => result,
            Err(error) => Err(M4Error::internal(format!(
                "M4 blocking operation failed to join: {error}"
            ))),
        };
        let trace = M4TraceSummary {
            trace_id,
            core_ms,
            total_ms: elapsed_ms(total_started),
        };
        self.record_trace(tool, &trace, result.is_ok());
        match result {
            Ok(value) => M4Outcome {
                ok: true,
                result: Some(value),
                error: None,
                trace,
            },
            Err(error) => M4Outcome {
                ok: false,
                result: None,
                error: Some(error),
                trace,
            },
        }
    }
    fn record_trace(&self, tool: &str, trace: &M4TraceSummary, ok: bool) {
        let Some(path) = &self.state.config.trace_path else {
            return;
        };
        let _guard = match self.state.trace_lock.lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!("M4 trace lock poisoned: {error}");
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
            tracing::warn!("cannot append M4 trace {}: {error}", path.display());
        }
    }
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
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum M4ReadMode {
    Full,
    Slice,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M4WorkspaceReadRequest {
    pub schema_version: u32,
    pub workspace_id: String,
    pub relative_path: String,
    pub mode: M4ReadMode,
    #[serde(default)]
    pub offset: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct M4WorkspaceReadResult {
    pub content: String,
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_byte_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eof: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M4WorkspaceDiffRequest {
    pub schema_version: u32,
    pub workspace_id: String,
    pub max_bytes: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeTaskProjection {
    schema_version: u32,
    task_id: String,
    stdout_tail_bytes: u64,
    stderr_tail_bytes: u64,
    ttl_ms: Option<u64>,
}

fn native_projection_root(config: &UniversalExecutorConfig) -> PathBuf {
    config.store_root.join("m4-native-task-projections")
}

fn native_projection_path(config: &UniversalExecutorConfig, task_id: &str) -> PathBuf {
    native_projection_root(config).join(format!("{task_id}.json"))
}

fn write_native_projection(
    config: &UniversalExecutorConfig,
    projection: &NativeTaskProjection,
) -> Result<(), M4Error> {
    let target = native_projection_path(config, &projection.task_id);
    let temporary = target.with_extension(format!("{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec(projection)
        .map_err(|error| M4Error::internal(format!("cannot encode task projection: {error}")))?;
    fs::write(&temporary, bytes)
        .map_err(|error| M4Error::internal(format!("cannot write task projection: {error}")))?;
    fs::rename(&temporary, &target)
        .map_err(|error| M4Error::internal(format!("cannot commit task projection: {error}")))?;
    Ok(())
}
fn read_native_projection(
    config: &UniversalExecutorConfig,
    task_id: &str,
) -> Result<NativeTaskProjection, M4Error> {
    let path = native_projection_path(config, task_id);
    let bytes = fs::read(&path).map_err(|error| {
        M4Error::invalid(
            format!("native MCP task projection is unavailable: {error}"),
            "taskId",
        )
    })?;
    let projection: NativeTaskProjection = serde_json::from_slice(&bytes)
        .map_err(|error| M4Error::internal(format!("invalid task projection: {error}")))?;
    if projection.task_id != task_id {
        return Err(M4Error::internal("task projection identity mismatch"));
    }
    Ok(projection)
}

fn parse_task_run(arguments: Option<JsonObject>) -> Result<TaskRunRequest, McpError> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(|error| {
        McpError::invalid_params(
            format!("invalid workspace.exec task arguments: {error}"),
            None,
        )
    })
}
#[tool_router]
impl M4Server {
    #[tool(
        name = "workspace.open",
        description = "Create one detached isolated Git workspace at an exact revision.",
        annotations(
            title = "Open isolated workspace",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_open(
        &self,
        Parameters(request): Parameters<GitWorkspaceCreateRequest>,
    ) -> M4Outcome<CompactWorkspaceOpenResult> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.open", move || {
            create_git_workspace_compact(&config, &request).map_err(M4Error::from)
        })
        .await
    }

    #[tool(
        name = "workspace.read",
        description = "Read bounded UTF-8 content from an isolated workspace in FULL or SLICE mode.",
        annotations(
            title = "Read workspace content",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_read(
        &self,
        Parameters(request): Parameters<M4WorkspaceReadRequest>,
    ) -> M4Outcome<M4WorkspaceReadResult> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.read", move || match request.mode {
            M4ReadMode::Full => {
                if request.offset != 0 {
                    return Err(M4Error::invalid(
                        "offset must be zero in FULL mode",
                        "offset",
                    ));
                }
                let result = read_workspace_text_compact(
                    &config,
                    &WorkspaceReadRequest {
                        schema_version: request.schema_version,
                        workspace_id: request.workspace_id,
                        relative_path: request.relative_path,
                        max_bytes: request.max_bytes,
                    },
                )
                .map_err(M4Error::from)?;
                Ok(M4WorkspaceReadResult {
                    content: result.content,
                    digest: result.digest,
                    file_byte_length: None,
                    eof: None,
                })
            }
            M4ReadMode::Slice => {
                let result = read_workspace_slice_compact(
                    &config,
                    &WorkspaceReadSliceRequest {
                        schema_version: request.schema_version,
                        workspace_id: request.workspace_id,
                        relative_path: request.relative_path,
                        offset: request.offset,
                        max_bytes: request.max_bytes,
                    },
                )
                .map_err(M4Error::from)?;
                Ok(M4WorkspaceReadResult {
                    content: result.content,
                    digest: result.file_digest,
                    file_byte_length: Some(result.file_byte_length),
                    eof: Some(result.eof),
                })
            }
        })
        .await
    }
    #[tool(
        name = "workspace.mutate",
        description = "Apply one validated batch of WRITE, APPEND, or exact replacement mutations.",
        annotations(
            title = "Mutate workspace files",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_mutate(
        &self,
        Parameters(request): Parameters<WorkspaceMutateRequest>,
    ) -> M4Outcome<WorkspaceMutateResult> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.mutate", move || {
            mutate_workspace(&config, &request).map_err(M4Error::from)
        })
        .await
    }

    #[tool(
        name = "workspace.diff",
        description = "Return a bounded compact Git diff and untracked paths for an isolated workspace.",
        annotations(
            title = "Inspect workspace diff",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_diff(
        &self,
        Parameters(request): Parameters<M4WorkspaceDiffRequest>,
    ) -> M4Outcome<CompactWorkspaceDiffResult> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.diff", move || {
            workspace_diff_compact(
                &config,
                &WorkspaceDiffRequest {
                    schema_version: request.schema_version,
                    workspace_id: request.workspace_id,
                    max_bytes: request.max_bytes,
                },
            )
            .map_err(M4Error::from)
        })
        .await
    }
    #[tool(
        name = "workspace.exec",
        description = "Run an absolute executable plus argv as a durable sandboxed Task. Small results return compactly; long work remains recoverable by Task ID.",
        execution(task_support = "optional"),
        annotations(
            title = "Execute durable workspace task",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_exec(
        &self,
        Parameters(request): Parameters<TaskRunRequest>,
    ) -> M4Outcome<CompactTaskObservation> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.exec", move || {
            run_universal_task_compact(&config, &request).map_err(M4Error::from)
        })
        .await
    }

    #[tool(
        name = "task.observe",
        description = "Observe or briefly await one durable Ordivon Task by Task ID.",
        annotations(
            title = "Observe durable task",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn task_observe(
        &self,
        Parameters(request): Parameters<TaskAwaitRequest>,
    ) -> M4Outcome<CompactTaskObservation> {
        let config = self.state.config.executor.clone();
        self.run_core("task.observe", move || {
            await_universal_task_compact(&config, &request).map_err(M4Error::from)
        })
        .await
    }
    #[tool(
        name = "task.cancel",
        description = "Cancel one durable Ordivon Task and stop its entire cgroup-owned process tree.",
        annotations(
            title = "Cancel durable task",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn task_cancel(
        &self,
        Parameters(request): Parameters<TaskCancelRequest>,
    ) -> M4Outcome<MigrationTaskHandle> {
        let config = self.state.config.executor.clone();
        self.run_core("task.cancel", move || {
            cancel_universal_task(&config, &request).map_err(M4Error::from)
        })
        .await
    }

    #[tool(
        name = "artifact.read",
        description = "Read a bounded UTF-8 range from stdout, stderr, or result Artifacts by stable Task and Artifact IDs.",
        annotations(
            title = "Read task artifact",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn artifact_read(
        &self,
        Parameters(request): Parameters<ArtifactReadRequest>,
    ) -> M4Outcome<ArtifactReadResult> {
        let config = self.state.config.executor.clone();
        self.run_core("artifact.read", move || {
            read_task_artifact(&config, &request).map_err(M4Error::from)
        })
        .await
    }
}
#[tool_handler]
impl ServerHandler for M4Server {
    fn get_info(&self) -> ServerInfo {
        let mut tools_task = ToolsTaskCapability::default();
        tools_task.call = Some(JsonObject::new());
        let mut requests = TaskRequestsCapability::default();
        requests.tools = Some(tools_task);
        let mut tasks = TasksCapability::default();
        tasks.requests = Some(requests);
        tasks.list = Some(JsonObject::new());
        tasks.cancel = Some(JsonObject::new());
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks_with(tasks)
                .build(),
        )
        .with_server_info(
            Implementation::new("ordivon-m4-experimental", env!("CARGO_PKG_VERSION"))
                .with_title("Ordivon Experimental MCP Adapter"),
        )
        .with_instructions(
            "Experimental localhost-only Ordivon adapter. Prefer compact workspace tools. workspace.exec supports native MCP Tasks or explicit Task handles. No production routing is authorized.",
        )
    }

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        if request.name.as_ref() != "workspace.exec" {
            return Err(McpError::invalid_params(
                "only workspace.exec supports task-based invocation",
                None,
            ));
        }
        let run_request = parse_task_run(request.arguments)?;
        let task_id = run_request.execution.task_id.clone();
        let projection = NativeTaskProjection {
            schema_version: 1,
            task_id: task_id.clone(),
            stdout_tail_bytes: run_request.stdout_tail_bytes,
            stderr_tail_bytes: run_request.stderr_tail_bytes,
            ttl_ms: request.task.and_then(|task| task.ttl),
        };
        let config = self.state.config.executor.clone();
        let run_for_start = run_request.clone();
        let start_result = tokio::task::spawn_blocking(move || {
            start_universal_task(&config, &run_for_start.execution).map_err(M4Error::from)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task enqueue: {error}"), None)
        })?;
        if let Err(error) = start_result {
            return Err(mcp_error_from_m4(error));
        }
        if let Err(error) = write_native_projection(&self.state.config.executor, &projection) {
            let cancel_config = self.state.config.executor.clone();
            let cancel_id = task_id.clone();
            let _ = tokio::task::spawn_blocking(move || {
                cancel_universal_task(
                    &cancel_config,
                    &TaskCancelRequest {
                        schema_version: 1,
                        task_id: cancel_id,
                    },
                )
            })
            .await;
            return Err(mcp_error_from_m4(error));
        }
        let task = self.load_mcp_task(&task_id).await?;
        Ok(CreateTaskResult::new(task))
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        self.load_mcp_task(&request.task_id)
            .await
            .map(GetTaskResult::new)
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        let protocol_started = Instant::now();
        let projection = read_native_projection(&self.state.config.executor, &request.task_id)
            .map_err(mcp_error_from_m4)?;
        let config = self.state.config.executor.clone();
        let task_id = request.task_id.clone();
        let observation = tokio::task::spawn_blocking(move || {
            await_universal_task_compact(
                &config,
                &TaskAwaitRequest {
                    schema_version: 1,
                    task_id,
                    wait_ms: 0,
                    stdout_tail_bytes: projection.stdout_tail_bytes,
                    stderr_tail_bytes: projection.stderr_tail_bytes,
                },
            )
            .map_err(M4Error::from)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task result load: {error}"), None)
        })?
        .map_err(mcp_error_from_m4)?;
        if observation.status == MigrationTaskStatus::Working {
            return Err(McpError::invalid_params("task result is not ready", None));
        }
        let ok = observation.status == MigrationTaskStatus::Completed;
        let error = match observation.status {
            MigrationTaskStatus::Completed => None,
            MigrationTaskStatus::Cancelled => Some(M4Error {
                code: "TASK_CANCELLED".to_string(),
                message: "task was cancelled".to_string(),
                field: Some("taskId".to_string()),
                retryable: false,
            }),
            MigrationTaskStatus::Failed => Some(M4Error {
                code: "TASK_FAILED".to_string(),
                message: observation
                    .error_summary
                    .clone()
                    .unwrap_or_else(|| "task failed".to_string()),
                field: Some("taskId".to_string()),
                retryable: false,
            }),
            _ => None,
        };
        let trace = M4TraceSummary {
            trace_id: format!("m4-task-result-{}", unix_ms()),
            core_ms: elapsed_ms(protocol_started),
            total_ms: elapsed_ms(protocol_started),
        };
        self.record_trace("tasks.result", &trace, ok);
        let outcome = M4Outcome {
            ok,
            result: Some(observation),
            error,
            trace,
        };
        let tool_result = outcome.into_call_tool_result()?;
        let value = serde_json::to_value(tool_result).map_err(|error| {
            McpError::internal_error(format!("cannot encode task result: {error}"), None)
        })?;
        Ok(GetTaskPayloadResult::new(value))
    }
    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        let config = self.state.config.executor.clone();
        let task_id = request.task_id.clone();
        tokio::task::spawn_blocking(move || {
            cancel_universal_task(
                &config,
                &TaskCancelRequest {
                    schema_version: 1,
                    task_id,
                },
            )
            .map_err(M4Error::from)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task cancellation: {error}"), None)
        })?
        .map_err(mcp_error_from_m4)?;
        self.load_mcp_task(&request.task_id)
            .await
            .map(CancelTaskResult::new)
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let cursor = request.and_then(|params| params.cursor);
        let config = self.state.config.executor.clone();
        let page = tokio::task::spawn_blocking(move || list_native_task_ids(&config, cursor))
            .await
            .map_err(|error| {
                McpError::internal_error(format!("cannot join task listing: {error}"), None)
            })?
            .map_err(mcp_error_from_m4)?;
        let mut tasks = Vec::with_capacity(page.0.len());
        for task_id in page.0 {
            tasks.push(self.load_mcp_task(&task_id).await?);
        }
        let mut result = ListTasksResult::new(tasks);
        result.next_cursor = page.1;
        Ok(result)
    }
}
impl M4Server {
    async fn load_mcp_task(&self, task_id: &str) -> Result<Task, McpError> {
        let config = self.state.config.executor.clone();
        let task_id_owned = task_id.to_string();
        let snapshot = tokio::task::spawn_blocking(move || {
            snapshot_universal_task(&config, &task_id_owned).map_err(M4Error::from)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task snapshot: {error}"), None)
        })?
        .map_err(mcp_error_from_m4)?;
        let ttl = read_native_projection(&self.state.config.executor, task_id)
            .map(|projection| projection.ttl_ms)
            .unwrap_or(None);
        task_from_snapshot(snapshot, ttl)
    }
}

fn task_from_snapshot(
    snapshot: DurableTaskSnapshot,
    ttl_ms: Option<u64>,
) -> Result<Task, McpError> {
    let mut task = Task::new(
        snapshot.task_id,
        mcp_task_status(snapshot.status),
        format_unix_ms(snapshot.created_unix_ms)?,
        format_unix_ms(snapshot.updated_unix_ms)?,
    )
    .with_status_message(snapshot.status_message);
    task.ttl = ttl_ms;
    if let Some(poll) = snapshot.poll_after_ms {
        task = task.with_poll_interval(poll);
    }
    Ok(task)
}

fn mcp_task_status(status: MigrationTaskStatus) -> TaskStatus {
    match status {
        MigrationTaskStatus::Working => TaskStatus::Working,
        MigrationTaskStatus::InputRequired => TaskStatus::InputRequired,
        MigrationTaskStatus::Completed => TaskStatus::Completed,
        MigrationTaskStatus::Failed => TaskStatus::Failed,
        MigrationTaskStatus::Cancelled => TaskStatus::Cancelled,
    }
}
fn format_unix_ms(value: u128) -> Result<String, McpError> {
    let nanos = value
        .checked_mul(1_000_000)
        .and_then(|value| i128::try_from(value).ok())
        .ok_or_else(|| McpError::internal_error("task timestamp is out of range", None))?;
    let timestamp = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|error| McpError::internal_error(format!("invalid task time: {error}"), None))?;
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| {
            McpError::internal_error(format!("cannot format task time: {error}"), None)
        })
}

fn mcp_error_from_m4(error: M4Error) -> McpError {
    let data = serde_json::to_value(&error).ok();
    if error.code == "TASK_NOT_FOUND" || error.code == "INVALID_REQUEST" {
        McpError::invalid_params(error.message, data)
    } else {
        McpError::internal_error(error.message, data)
    }
}

fn list_native_task_ids(
    config: &UniversalExecutorConfig,
    cursor: Option<String>,
) -> Result<(Vec<String>, Option<String>), M4Error> {
    let root = native_projection_root(config);
    let mut ids = Vec::new();
    for entry in fs::read_dir(&root)
        .map_err(|error| M4Error::internal(format!("cannot list task projections: {error}")))?
    {
        let entry = entry.map_err(|error| {
            M4Error::internal(format!("cannot read task projection entry: {error}"))
        })?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
            if cursor.as_ref().is_none_or(|cursor| stem > cursor.as_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    ids.sort();
    let next_cursor = (ids.len() > 100).then(|| ids[99].clone());
    ids.truncate(100);
    Ok((ids, next_cursor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Sandbox {
        root: PathBuf,
    }

    impl Sandbox {
        fn new(label: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "ordivon-m4-{label}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn server(&self) -> M4Server {
            M4Server::new(M4ServerConfig {
                executor: UniversalExecutorConfig {
                    store_root: self.root.join("store"),
                    runner_path: PathBuf::from("/usr/bin/true"),
                    allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
                    max_runtime_ms: 10_000,
                    max_output_bytes: 1024 * 1024,
                },
                trace_path: None,
            })
            .unwrap()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn tool_catalog_is_thin_and_exec_is_optional_task() {
        let sandbox = Sandbox::new("catalog");
        let server = sandbox.server();
        let mut tools = server.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(
            names,
            [
                "artifact.read",
                "task.cancel",
                "task.observe",
                "workspace.diff",
                "workspace.exec",
                "workspace.mutate",
                "workspace.open",
                "workspace.read",
            ]
        );
        let exec = tools
            .iter()
            .find(|tool| tool.name.as_ref() == "workspace.exec")
            .unwrap();
        assert_eq!(exec.task_support(), TaskSupport::Optional);
        assert!(tools
            .iter()
            .filter(|tool| tool.name.as_ref() != "workspace.exec")
            .all(|tool| tool.task_support() == TaskSupport::Forbidden));
    }

    #[test]
    fn structured_failure_is_a_tool_error_not_protocol_failure() {
        let outcome = M4Outcome::<String> {
            ok: false,
            result: None,
            error: Some(M4Error::invalid("digest mismatch", "expectedDigest")),
            trace: M4TraceSummary {
                trace_id: "trace-1".to_string(),
                core_ms: 2,
                total_ms: 3,
            },
        };
        let result = outcome.into_call_tool_result().unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result
                .structured_content
                .as_ref()
                .and_then(|value| value.get("error"))
                .and_then(|value| value.get("field"))
                .and_then(Value::as_str),
            Some("expectedDigest")
        );
    }

    #[test]
    fn native_task_projection_paginates_without_becoming_task_truth() {
        let sandbox = Sandbox::new("pagination");
        let server = sandbox.server();
        for index in 0..101 {
            write_native_projection(
                &server.state.config.executor,
                &NativeTaskProjection {
                    schema_version: 1,
                    task_id: format!("task-{index:03}"),
                    stdout_tail_bytes: 128,
                    stderr_tail_bytes: 128,
                    ttl_ms: Some(60_000),
                },
            )
            .unwrap();
        }
        let first = list_native_task_ids(&server.state.config.executor, None).unwrap();
        assert_eq!(first.0.len(), 100);
        assert_eq!(first.1.as_deref(), Some("task-099"));
        let second = list_native_task_ids(&server.state.config.executor, first.1.clone()).unwrap();
        assert_eq!(second.0, vec!["task-100"]);
        assert_eq!(second.1, None);
        assert!(!server
            .state
            .config
            .executor
            .tasks_root()
            .join("task-000")
            .exists());
    }

    #[test]
    fn durable_snapshot_maps_to_protocol_task_without_session_state() {
        let task = task_from_snapshot(
            DurableTaskSnapshot {
                task_id: "task-stable".to_string(),
                status: MigrationTaskStatus::Working,
                status_message: "working".to_string(),
                created_unix_ms: 1_700_000_000_000,
                updated_unix_ms: 1_700_000_000_100,
                poll_after_ms: Some(250),
                result_available: false,
            },
            Some(60_000),
        )
        .unwrap();
        assert_eq!(task.task_id, "task-stable");
        assert_eq!(task.status, TaskStatus::Working);
        assert_eq!(task.poll_interval, Some(250));
        assert_eq!(task.ttl, Some(60_000));
        assert!(task.created_at.starts_with("2023-"));
    }
}
