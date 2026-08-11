mod config;
mod error;
mod fsutil;
mod mutation;
mod patch;
mod projection;
mod runner;
mod types;
mod workspace;

pub use config::{UniversalExecutorConfig, MAX_WORKSPACE_IO_BYTES, UNIVERSAL_EXEC_SCHEMA_VERSION};
pub use error::{UniversalExecError, UniversalExecErrorCode};
pub use fsutil::{
    ENVIRONMENT_VARIABLE_NAME_PATTERN, WORKSPACE_ID_MAX_LENGTH, WORKSPACE_ID_MIN_LENGTH,
    WORKSPACE_ID_PATTERN,
};
pub use mutation::{mutate_workspace, read_workspace_slice};
pub use patch::{
    inspect_workspace_patch_plan, patch_workspace, plan_workspace_patch,
    result_from_workspace_patch_plan, WorkspacePatchPlan, WorkspacePatchPlanFile,
    WorkspacePatchPlanState,
};
pub use projection::{
    create_git_workspace_compact, read_workspace_slice_compact, read_workspace_text_compact,
    workspace_diff_compact,
};
pub use runner::run_task_runner;
pub use types::{
    CompactWorkspaceDiffResult, CompactWorkspaceOpenResult, CompactWorkspaceReadResult,
    CompactWorkspaceSliceResult, GitWorkspaceCreateRequest, WorkspaceChangeCursor,
    WorkspaceChangeEntry, WorkspaceChangeKind, WorkspaceChangePageRequest,
    WorkspaceChangePageResult, WorkspaceCloseRequest, WorkspaceCloseResult,
    WorkspaceClosureDisposition, WorkspaceContentMetadata, WorkspaceContentReadResult,
    WorkspaceContentRequest, WorkspaceDiffRequest, WorkspaceDiffResult, WorkspaceFilePatch,
    WorkspaceMutateRequest, WorkspaceMutateResult, WorkspaceMutation, WorkspaceMutationMode,
    WorkspaceMutationResult, WorkspacePatchRequest, WorkspacePatchResult, WorkspacePatchedFile,
    WorkspaceReadRequest, WorkspaceReadResult, WorkspaceReadSliceRequest, WorkspaceReadSliceResult,
    WorkspaceRecord, WorkspaceRenamedPath, WorkspaceTextEdit, WorkspaceTextPosition,
    WorkspaceTextRange, WorkspaceWriteRequest, WorkspaceWriteResult,
    MAX_WORKSPACE_CHANGE_PAGE_ENTRIES,
};
pub use workspace::{
    create_git_workspace, list_workspace_records, load_workspace_record, read_workspace_content,
    read_workspace_text, remove_git_workspace, workspace_changes_page, workspace_diff,
    write_workspace_text,
};
#[cfg(any(feature = "transactional-runtime", test))]
pub use workspace::{workspace_head_revision, workspace_is_dirty, workspace_source_state_digest};

pub(crate) use config::canonical_directory;
pub(crate) use fsutil::{
    invalid, io_error, now_unix_ms, open_directory_nofollow, open_regular_file_beneath,
    sha256_bytes, sha256_file, validate_env, validate_exec_payload, validate_id,
    validate_relative_path, write_bytes_atomic, write_json_atomic,
};
#[cfg(test)]
pub(crate) use fsutil::{
    linux_exec_payload_limit_bytes, linux_exec_string_limit_bytes, validate_args,
};
pub(crate) use types::{
    CapturedOutput, RunnerExecutionStep, RunnerHostDependencyCommitment, RunnerInputCommitment,
    RunnerPayloadConfig, RunnerStartEvidence, RunnerStepResult, RunnerTaskProgress,
    RunnerTaskRequest, RunnerTaskResult, TaskTerminalStatus,
};
#[cfg(feature = "transactional-runtime")]
pub(crate) use workspace::resolve_workspace_cwd;
pub(crate) use workspace::{
    list_open_workspace_record_inventory, preflight_workspace_write_path, remove_workspace_file,
    resolve_existing_workspace_path, workspace_change_projection_at, workspace_cleanup_dependents,
    workspace_diff_paths, workspace_git_common_dir_at, workspace_head_and_dirty_at,
    workspace_head_revision_at, workspace_source_state_digest_at,
};

#[cfg(test)]
mod tests;
