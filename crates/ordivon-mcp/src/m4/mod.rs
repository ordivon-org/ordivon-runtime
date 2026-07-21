use std::borrow::Cow;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
static GLOBAL_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static GLOBAL_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone)]
pub struct M4ServerConfig {
    pub executor: UniversalExecutorConfig,
    pub trace_path: Option<PathBuf>,
    pub dogfood_policy: Option<M5DogfoodPolicy>,
}

#[derive(Clone, Debug)]
pub struct M5DogfoodPolicy {
    pub allowed_source_repos: Vec<PathBuf>,
    pub allowed_source_revisions: Vec<String>,
}

impl M5DogfoodPolicy {
    pub(crate) fn canonicalized(self) -> Result<Self, M4Error> {
        if self.allowed_source_repos.is_empty() {
            return Err(M4Error::invalid(
                "M5 dogfood requires at least one source repository",
                "allowedSourceRepos",
            ));
        }
        if self.allowed_source_revisions.is_empty() {
            return Err(M4Error::invalid(
                "M5 dogfood requires at least one source revision",
                "allowedSourceRevisions",
            ));
        }
        let allowed_source_repos = self
            .allowed_source_repos
            .into_iter()
            .map(|path| {
                std::fs::canonicalize(&path).map_err(|error| {
                    M4Error::invalid(
                        format!(
                            "cannot canonicalize allowed source repo {}: {error}",
                            path.display()
                        ),
                        "allowedSourceRepos",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            allowed_source_repos,
            allowed_source_revisions: self.allowed_source_revisions,
        })
    }

    pub(crate) fn authorize(&self, request: &GitWorkspaceCreateRequest) -> Result<(), M4Error> {
        let source_repo =
            std::fs::canonicalize(Path::new(&request.source_repo)).map_err(|error| {
                M4Error::invalid(
                    format!("cannot canonicalize sourceRepo: {error}"),
                    "sourceRepo",
                )
            })?;
        if !self.allowed_source_repos.contains(&source_repo) {
            return Err(M4Error::forbidden(
                "SOURCE_REPO_NOT_ALLOWED",
                "sourceRepo is outside the M5 dogfood allowlist",
                "sourceRepo",
            ));
        }
        if !self
            .allowed_source_revisions
            .iter()
            .any(|revision| revision == &request.source_revision)
        {
            return Err(M4Error::forbidden(
                "SOURCE_REVISION_NOT_ALLOWED",
                "sourceRevision is outside the M5 dogfood allowlist",
                "sourceRevision",
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct M4Server {
    state: Arc<M4State>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

struct M4State {
    config: M4ServerConfig,
}

impl M4Server {
    pub fn new(mut config: M4ServerConfig) -> Result<Self, M4Error> {
        config.executor.ensure_store().map_err(M4Error::from)?;
        config.dogfood_policy = config
            .dogfood_policy
            .take()
            .map(M5DogfoodPolicy::canonicalized)
            .transpose()?;
        fs::create_dir_all(native_projection_root(&config.executor)).map_err(|error| {
            M4Error::internal(format!("cannot create M4 projection store: {error}"))
        })?;
        let state = Arc::new(M4State { config });
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

    fn forbidden(code: &str, message: impl Into<String>, field: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            field: Some(field.to_string()),
            retryable: false,
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

#[derive(Clone, Debug)]
pub enum M4Outcome<T> {
    Success(T),
    Error(M4Error),
}

impl<T: JsonSchema> JsonSchema for M4Outcome<T> {
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

impl<T> IntoCallToolResult for M4Outcome<T>
where
    T: Serialize + JsonSchema + Send + 'static,
{
    fn into_call_tool_result(self) -> Result<CallToolResult, McpError> {
        let (ok, value, compatibility_text) = match self {
            Self::Success(result) => {
                let value = serde_json::to_value(result).map_err(|error| {
                    McpError::internal_error(format!("cannot serialize M4 result: {error}"), None)
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

impl M4Server {
    async fn run_core<T, F>(&self, tool: &'static str, operation: F) -> M4Outcome<T>
    where
        T: Serialize + JsonSchema + Send + 'static,
        F: FnOnce() -> Result<T, M4Error> + Send + 'static,
    {
        let trace_id = next_trace_id("core");
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
            Ok(value) => M4Outcome::Success(value),
            Err(error) => M4Outcome::Error(error),
        }
    }
    fn record_trace(&self, tool: &str, trace: &M4TraceSummary, ok: bool) {
        let Some(path) = &self.state.config.trace_path else {
            return;
        };
        let _guard = match GLOBAL_TRACE_LOCK.get_or_init(|| Mutex::new(())).lock() {
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

fn next_trace_id(kind: &str) -> String {
    format!(
        "m4-{kind}-{}-{}-{}",
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

mod tasks;
mod tools;

#[cfg(test)]
mod tests;
