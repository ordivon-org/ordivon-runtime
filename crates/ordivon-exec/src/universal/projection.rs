use super::{
    create_git_workspace, read_workspace_slice, read_workspace_text, workspace_diff,
    CompactWorkspaceDiffResult, CompactWorkspaceOpenResult, CompactWorkspaceReadResult,
    CompactWorkspaceSliceResult, GitWorkspaceCreateRequest, UniversalExecError,
    UniversalExecutorConfig, WorkspaceDiffRequest, WorkspaceReadRequest, WorkspaceReadSliceRequest,
};

pub fn create_git_workspace_compact(
    config: &UniversalExecutorConfig,
    request: &GitWorkspaceCreateRequest,
) -> Result<CompactWorkspaceOpenResult, UniversalExecError> {
    let record = create_git_workspace(config, request)?;
    Ok(CompactWorkspaceOpenResult {
        workspace_id: record.workspace_id,
        source_revision: record.source_revision,
    })
}

pub fn read_workspace_text_compact(
    config: &UniversalExecutorConfig,
    request: &WorkspaceReadRequest,
) -> Result<CompactWorkspaceReadResult, UniversalExecError> {
    let result = read_workspace_text(config, request)?;
    Ok(CompactWorkspaceReadResult {
        content: result.content,
        digest: result.digest,
    })
}
pub fn workspace_diff_compact(
    config: &UniversalExecutorConfig,
    request: &WorkspaceDiffRequest,
) -> Result<CompactWorkspaceDiffResult, UniversalExecError> {
    let result = workspace_diff(config, request)?;
    Ok(CompactWorkspaceDiffResult {
        diff: result.diff,
        truncated: result.truncated,
        untracked_paths: result.untracked_paths,
    })
}

pub fn read_workspace_slice_compact(
    config: &UniversalExecutorConfig,
    request: &WorkspaceReadSliceRequest,
) -> Result<CompactWorkspaceSliceResult, UniversalExecError> {
    let result = read_workspace_slice(config, request)?;
    Ok(CompactWorkspaceSliceResult {
        content: result.content,
        file_digest: result.file_digest,
        file_byte_length: result.file_byte_length,
        eof: result.eof,
    })
}
