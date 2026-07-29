mod config;
mod error;
mod fsutil;
mod mutation;
mod patch;
mod projection;
mod runner;
mod types;
mod workspace;

pub use config::{
    UniversalExecutorConfig, MAX_UNIVERSAL_ARGS, MAX_UNIVERSAL_ARG_BYTES,
    MAX_UNIVERSAL_ENV_VALUE_BYTES, MAX_UNIVERSAL_ENV_VARS, MAX_UNIVERSAL_OUTPUT_BYTES,
    MAX_UNIVERSAL_RUNTIME_MS, MAX_WORKSPACE_IO_BYTES, MAX_WORKSPACE_MUTATIONS,
    UNIVERSAL_EXEC_SCHEMA_VERSION,
};
pub use error::{UniversalExecError, UniversalExecErrorCode};
pub use mutation::{mutate_workspace, read_workspace_slice};
pub use patch::patch_workspace;
pub use projection::{
    create_git_workspace_compact, read_workspace_slice_compact, read_workspace_text_compact,
    workspace_diff_compact,
};
pub use runner::run_task_runner;
pub use types::{
    CompactWorkspaceDiffResult, CompactWorkspaceOpenResult, CompactWorkspaceReadResult,
    CompactWorkspaceSliceResult, GitWorkspaceCreateRequest, WorkspaceCloseRequest,
    WorkspaceCloseResult, WorkspaceDiffRequest, WorkspaceDiffResult, WorkspaceFilePatch,
    WorkspaceMutateRequest, WorkspaceMutateResult, WorkspaceMutation, WorkspaceMutationMode,
    WorkspaceMutationResult, WorkspacePatchRequest, WorkspacePatchResult, WorkspacePatchedFile,
    WorkspaceReadRequest, WorkspaceReadResult, WorkspaceReadSliceRequest, WorkspaceReadSliceResult,
    WorkspaceRecord, WorkspaceTextEdit, WorkspaceTextPosition, WorkspaceTextRange,
    WorkspaceWriteRequest, WorkspaceWriteResult, MAX_WORKSPACE_PATCH_EDITS_PER_FILE,
    MAX_WORKSPACE_PATCH_FILES,
};
#[cfg(any(feature = "transactional-runtime", test))]
pub use workspace::workspace_source_state_digest;
pub use workspace::{
    create_git_workspace, list_workspace_records, load_workspace_record, read_workspace_text,
    remove_git_workspace, workspace_diff, write_workspace_text,
};

pub(crate) use config::canonical_directory;
pub(crate) use fsutil::{
    invalid, io_error, now_unix_ms, sha256_bytes, sha256_file, validate_args, validate_env,
    validate_id, validate_relative_path, write_bytes_atomic, write_json_atomic,
};
pub(crate) use types::{
    CapturedOutput, RunnerExecutionStep, RunnerPayloadConfig, RunnerStartEvidence,
    RunnerStepResult, RunnerTaskProgress, RunnerTaskRequest, RunnerTaskResult, TaskTerminalStatus,
};
#[cfg(feature = "transactional-runtime")]
pub(crate) use workspace::resolve_workspace_cwd;
pub(crate) use workspace::{
    preflight_workspace_write_path, remove_workspace_file, resolve_existing_workspace_path,
    workspace_diff_paths, workspace_git_common_dir_at, workspace_source_state_digest_at,
};

#[cfg(test)]
mod tests;
