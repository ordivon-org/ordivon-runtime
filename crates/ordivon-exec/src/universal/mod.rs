mod config;
mod error;
mod fsutil;
mod runner;
mod task;
mod types;
mod workspace;

pub use config::{
    UniversalExecutorConfig, MAX_ARTIFACT_READ_BYTES, MAX_TASK_WAIT_MS, MAX_UNIVERSAL_ARGS,
    MAX_UNIVERSAL_ARG_BYTES, MAX_UNIVERSAL_ENV_VALUE_BYTES, MAX_UNIVERSAL_ENV_VARS,
    MAX_UNIVERSAL_OUTPUT_BYTES, MAX_UNIVERSAL_RUNTIME_MS, MAX_WORKSPACE_IO_BYTES,
    UNIVERSAL_EXEC_SCHEMA_VERSION,
};
pub use error::{UniversalExecError, UniversalExecErrorCode};
pub use runner::run_task_runner;
pub use task::{
    cancel_universal_task, get_universal_task, read_task_artifact, start_universal_task,
};
pub use types::{
    ArtifactReadRequest, ArtifactReadResult, GitWorkspaceCreateRequest, TaskCancelRequest,
    TaskGetRequest, UniversalExecRequest, WorkspaceDiffRequest, WorkspaceDiffResult,
    WorkspaceReadRequest, WorkspaceReadResult, WorkspaceRecord, WorkspaceWriteRequest,
    WorkspaceWriteResult,
};
pub use workspace::{
    create_git_workspace, load_workspace_record, read_workspace_text, remove_git_workspace,
    workspace_diff, write_workspace_text,
};

pub(crate) use config::canonical_directory;
pub(crate) use fsutil::{
    invalid, io_error, now_unix_ms, sha256_bytes, sha256_file, validate_args, validate_artifact_id,
    validate_env, validate_id, validate_relative_path, write_bytes_atomic, write_json_atomic,
};
pub(crate) use types::{
    CapturedOutput, RunnerTaskRequest, RunnerTaskResult, TaskMetadata, TaskTerminalStatus,
};
pub(crate) use workspace::resolve_workspace_cwd;

#[cfg(test)]
mod tests;
