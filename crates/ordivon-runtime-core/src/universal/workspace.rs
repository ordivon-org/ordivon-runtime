use std::collections::BTreeMap;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{
    canonical_directory, invalid, io_error, now_unix_ms, sha256_bytes, sha256_file,
    validate_relative_path, write_bytes_atomic, write_json_atomic, GitWorkspaceCreateRequest,
    UniversalExecError, UniversalExecErrorCode, UniversalExecutorConfig, WorkspaceCloseRequest,
    WorkspaceCloseResult, WorkspaceDiffRequest, WorkspaceDiffResult, WorkspaceReadRequest,
    WorkspaceReadResult, WorkspaceRecord, WorkspaceWriteRequest, WorkspaceWriteResult,
    UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClosedWorkspaceRecord {
    schema_version: u32,
    state: String,
    workspace_id: String,
    source_repo: String,
    source_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_head: Option<String>,
    closed_unix_ms: u128,
    removal_result: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceSourceState {
    schema_version: u32,
    head_revision: String,
    index_digest: String,
    tracked: Vec<WorkspaceSourceEntry>,
    untracked: Vec<WorkspaceSourceEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceSourceEntry {
    path: String,
    kind: String,
    mode: u32,
    byte_length: u64,
    digest: String,
}

pub fn create_git_workspace(
    config: &UniversalExecutorConfig,
    request: &GitWorkspaceCreateRequest,
) -> Result<WorkspaceRecord, UniversalExecError> {
    config.ensure_store()?;
    request.validate_shape()?;
    let target = config.workspace_path(&request.workspace_id);
    let record_path = config.workspace_record_path(&request.workspace_id);
    if target.exists() || record_path.exists() {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceExists,
            "workspace already exists",
            Some("workspaceId"),
            false,
        ));
    }
    let source_repo = canonical_directory(Path::new(&request.source_repo), "sourceRepo")?;
    let revision = git_output(
        &source_repo,
        [
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", request.source_revision),
        ],
    )?;
    let revision = revision.trim().to_string();
    if revision.len() != 40 && revision.len() != 64 {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::RevisionNotFound,
            "source revision did not resolve to a commit",
            Some("sourceRevision"),
            false,
        ));
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&source_repo)
        .args(["worktree", "add", "--detach"])
        .arg(&target)
        .arg(&revision)
        .output()
        .map_err(|error| tool_unavailable("git worktree add", error))?;
    if !output.status.success() {
        return Err(tool_failed("git worktree add", &output.stderr));
    }
    let canonical_target = canonical_directory(&target, "workspacePath")?;
    let actual_revision = git_output(&canonical_target, ["rev-parse", "HEAD"])?;
    if actual_revision.trim() != revision {
        let _ = remove_git_worktree(&source_repo, &canonical_target, true);
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::RevisionMismatch,
            "created workspace HEAD does not match requested revision",
            Some("sourceRevision"),
            false,
        ));
    }
    if let (Some(uid), Some(gid)) = (config.workspace_uid, config.workspace_gid) {
        if let Err(error) = transfer_workspace_ownership(&canonical_target, uid, gid) {
            let _ = remove_git_worktree(&source_repo, &canonical_target, true);
            return Err(error);
        }
    }
    let record = WorkspaceRecord {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: request.workspace_id.clone(),
        source_repo: source_repo.to_string_lossy().into_owned(),
        source_revision: revision,
        workspace_path: canonical_target.to_string_lossy().into_owned(),
        created_unix_ms: now_unix_ms()?,
    };
    if let Err(error) = write_json_atomic(&record_path, &record) {
        let _ = remove_git_worktree(&source_repo, &canonical_target, true);
        return Err(error);
    }
    Ok(record)
}

pub fn load_workspace_record(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
) -> Result<WorkspaceRecord, UniversalExecError> {
    let record = load_workspace_record_metadata(config, workspace_id)?;
    let expected = canonical_directory(&config.workspace_path(workspace_id), "workspacePath")?;
    let recorded = canonical_directory(Path::new(&record.workspace_path), "workspacePath")?;
    if expected != recorded {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            "workspace record path mismatch",
            Some("workspacePath"),
            false,
        ));
    }
    Ok(record)
}

fn load_workspace_record_metadata(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
) -> Result<WorkspaceRecord, UniversalExecError> {
    super::validate_id(workspace_id, "workspaceId")?;
    let path = config.workspace_record_path(workspace_id);
    let bytes = read_workspace_record_bytes(&path)?;
    if let Some(closed) = decode_closed_workspace_record(&bytes)? {
        validate_closed_identity(&closed, workspace_id)?;
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceNotFound,
            "workspace is closed",
            Some("workspaceId"),
            false,
        ));
    }
    decode_open_workspace_record(&bytes, workspace_id)
}

fn read_workspace_record_bytes(path: &Path) -> Result<Vec<u8>, UniversalExecError> {
    fs::read(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceNotFound,
            format!("cannot read workspace record: {error}"),
            Some("workspaceId"),
            false,
        )
    })
}

fn decode_closed_workspace_record(
    bytes: &[u8],
) -> Result<Option<ClosedWorkspaceRecord>, UniversalExecError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("invalid workspace record: {error}"),
            Some("workspaceId"),
            false,
        )
    })?;
    if value.get("state").and_then(serde_json::Value::as_str) != Some("closed") {
        return Ok(None);
    }
    serde_json::from_value(value).map(Some).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("invalid closed workspace record: {error}"),
            Some("workspaceId"),
            false,
        )
    })
}

fn decode_open_workspace_record(
    bytes: &[u8],
    workspace_id: &str,
) -> Result<WorkspaceRecord, UniversalExecError> {
    let record: WorkspaceRecord = serde_json::from_slice(bytes).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("invalid workspace record: {error}"),
            Some("workspaceId"),
            false,
        )
    })?;
    validate_open_identity(&record, workspace_id)?;
    Ok(record)
}

fn validate_open_identity(
    record: &WorkspaceRecord,
    workspace_id: &str,
) -> Result<(), UniversalExecError> {
    if record.schema_version != UNIVERSAL_EXEC_SCHEMA_VERSION || record.workspace_id != workspace_id
    {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            "workspace record identity mismatch",
            Some("workspaceId"),
            false,
        ));
    }
    Ok(())
}

fn validate_closed_identity(
    record: &ClosedWorkspaceRecord,
    workspace_id: &str,
) -> Result<(), UniversalExecError> {
    if record.schema_version != UNIVERSAL_EXEC_SCHEMA_VERSION
        || record.state != "closed"
        || record.workspace_id != workspace_id
    {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            "closed workspace record identity mismatch",
            Some("workspaceId"),
            false,
        ));
    }
    Ok(())
}

pub fn list_workspace_records(
    config: &UniversalExecutorConfig,
    limit: u32,
) -> Result<Vec<WorkspaceRecord>, UniversalExecError> {
    config.ensure_store()?;
    if limit == 0 || limit > 100 {
        return Err(invalid("limit must be in 1..=100", "limit"));
    }
    let records_root = config.workspace_records_root();
    let mut records = Vec::new();
    for entry in
        fs::read_dir(&records_root).map_err(|error| io_error(&records_root, "list", error))?
    {
        let entry =
            entry.map_err(|error| io_error(&records_root, "read directory entry", error))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(workspace_id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        match load_workspace_record_metadata(config, workspace_id) {
            Ok(record) => {
                let expected_path = config.workspace_path(workspace_id);
                if Path::new(&record.workspace_path) != expected_path {
                    return Err(UniversalExecError::new(
                        UniversalExecErrorCode::MetadataCorrupt,
                        "workspace record path does not match its identity",
                        Some("workspaceId"),
                        false,
                    ));
                }
                match fs::symlink_metadata(&expected_path) {
                    Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                        records.push(record);
                    }
                    Ok(_) => {
                        return Err(UniversalExecError::new(
                            UniversalExecErrorCode::MetadataCorrupt,
                            "workspace record target must be a non-symlink directory",
                            Some("workspaceId"),
                            false,
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(io_error(&expected_path, "inspect", error)),
                }
            }
            Err(error) if error.code == UniversalExecErrorCode::WorkspaceNotFound => continue,
            Err(error) => return Err(error),
        }
    }
    records.sort_by(|left, right| {
        right
            .created_unix_ms
            .cmp(&left.created_unix_ms)
            .then_with(|| left.workspace_id.cmp(&right.workspace_id))
    });
    records.truncate(limit as usize);
    Ok(records)
}

pub fn read_workspace_text(
    config: &UniversalExecutorConfig,
    request: &WorkspaceReadRequest,
) -> Result<WorkspaceReadResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let path = resolve_existing_workspace_path(&record, &request.relative_path, false)?;
    let metadata = fs::metadata(&path).map_err(|error| io_error(&path, "inspect", error))?;
    if !metadata.is_file() {
        return Err(invalid(
            "relativePath must resolve to a file",
            "relativePath",
        ));
    }
    if metadata.len() > request.max_bytes {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::OutputLimitExceeded,
            format!("file exceeds maxBytes {}", request.max_bytes),
            Some("maxBytes"),
            false,
        ));
    }
    let bytes = fs::read(&path).map_err(|error| io_error(&path, "read", error))?;
    let content = String::from_utf8(bytes.clone()).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            format!("workspace file is not UTF-8: {error}"),
            Some("relativePath"),
            false,
        )
    })?;
    Ok(WorkspaceReadResult {
        workspace_id: request.workspace_id.clone(),
        relative_path: request.relative_path.clone(),
        content,
        digest: sha256_bytes(&bytes),
        byte_length: bytes.len() as u64,
    })
}

pub fn write_workspace_text(
    config: &UniversalExecutorConfig,
    request: &WorkspaceWriteRequest,
) -> Result<WorkspaceWriteResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let path = resolve_workspace_write_path(&record, &request.relative_path)?;
    let before_digest = if path.exists() {
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| io_error(&path, "inspect", error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::WorkspacePathDenied,
                "write target must be a non-symlink regular file",
                Some("relativePath"),
                false,
            ));
        }
        Some(sha256_file(&path)?)
    } else {
        None
    };
    if request.expected_digest != before_digest
        && (request.expected_digest.is_some() || before_digest.is_some())
    {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::RevisionMismatch,
            "workspace file digest does not match expectedDigest",
            Some("expectedDigest"),
            false,
        ));
    }
    let existing_permissions = fs::metadata(&path)
        .ok()
        .map(|metadata| metadata.permissions());
    write_bytes_atomic(&path, request.content.as_bytes())?;
    if let Some(permissions) = existing_permissions {
        fs::set_permissions(&path, permissions)
            .map_err(|error| io_error(&path, "set permissions", error))?;
    } else {
        let mut permissions = fs::metadata(&path)
            .map_err(|error| io_error(&path, "inspect", error))?
            .permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&path, permissions)
            .map_err(|error| io_error(&path, "set permissions", error))?;
    }
    Ok(WorkspaceWriteResult {
        workspace_id: request.workspace_id.clone(),
        relative_path: request.relative_path.clone(),
        before_digest,
        after_digest: sha256_file(&path)?,
        byte_length: request.content.len() as u64,
    })
}

pub fn workspace_diff(
    config: &UniversalExecutorConfig,
    request: &WorkspaceDiffRequest,
) -> Result<WorkspaceDiffResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let workspace = Path::new(&record.workspace_path);
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["diff", "HEAD", "--no-ext-diff", "--no-color", "--binary"])
        .output()
        .map_err(|error| tool_unavailable("git diff", error))?;
    if !output.status.success() {
        return Err(tool_failed("git diff", &output.stderr));
    }
    let total = output.stdout.len();
    let retained = total.min(request.max_bytes as usize);
    let bytes = &output.stdout[..retained];
    let diff = String::from_utf8(bytes.to_vec()).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            format!("git diff output is not UTF-8: {error}"),
            None,
            false,
        )
    })?;
    let untracked_output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .output()
        .map_err(|error| tool_unavailable("git ls-files", error))?;
    if !untracked_output.status.success() {
        return Err(tool_failed("git ls-files", &untracked_output.stderr));
    }
    let mut untracked_paths = Vec::new();
    for raw in untracked_output.stdout.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        if untracked_paths.len() >= 256 {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::OutputLimitExceeded,
                "workspace has more than 256 untracked paths",
                None,
                false,
            ));
        }
        let path = String::from_utf8(raw.to_vec()).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ArtifactNotUtf8,
                format!("untracked Git path is not UTF-8: {error}"),
                None,
                false,
            )
        })?;
        untracked_paths.push(path);
    }
    Ok(WorkspaceDiffResult {
        workspace_id: request.workspace_id.clone(),
        diff,
        digest: sha256_bytes(bytes),
        byte_length: retained as u64,
        truncated: retained < total,
        untracked_paths,
    })
}

pub(crate) fn workspace_diff_paths(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
    relative_paths: &[&str],
    max_bytes: u64,
) -> Result<(String, bool), UniversalExecError> {
    let record = load_workspace_record(config, workspace_id)?;
    let workspace = Path::new(&record.workspace_path);
    let mut combined = Vec::new();
    for relative_path in relative_paths {
        let tracked = Command::new("git")
            .arg("-C")
            .arg(workspace)
            .args(["ls-files", "--error-unmatch", "--"])
            .arg(relative_path)
            .output()
            .map_err(|error| tool_unavailable("git ls-files", error))?;
        let output = if tracked.status.success() {
            Command::new("git")
                .arg("-C")
                .arg(workspace)
                .args([
                    "diff",
                    "HEAD",
                    "--no-ext-diff",
                    "--no-color",
                    "--binary",
                    "--",
                ])
                .arg(relative_path)
                .output()
                .map_err(|error| tool_unavailable("git diff", error))?
        } else {
            let output = Command::new("git")
                .arg("-C")
                .arg(workspace)
                .args([
                    "diff",
                    "--no-index",
                    "--no-color",
                    "--binary",
                    "--",
                    "/dev/null",
                ])
                .arg(relative_path)
                .output()
                .map_err(|error| tool_unavailable("git diff --no-index", error))?;
            if !output.status.success() && output.status.code() != Some(1) {
                return Err(tool_failed("git diff --no-index", &output.stderr));
            }
            output
        };
        if tracked.status.success() && !output.status.success() {
            return Err(tool_failed("git diff", &output.stderr));
        }
        combined.extend_from_slice(&output.stdout);
    }
    let total = combined.len();
    let retained = total.min(max_bytes as usize);
    let diff = String::from_utf8(combined[..retained].to_vec()).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            format!("git diff output is not UTF-8: {error}"),
            None,
            false,
        )
    })?;
    Ok((diff, retained < total))
}

#[cfg(any(feature = "transactional-runtime", test))]
pub fn workspace_source_state_digest(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
) -> Result<String, UniversalExecError> {
    let record = load_workspace_record(config, workspace_id)?;
    workspace_source_state_digest_at(Path::new(&record.workspace_path))
}

pub(crate) fn workspace_source_state_digest_at(
    workspace: &Path,
) -> Result<String, UniversalExecError> {
    let workspace = canonical_directory(workspace, "workspacePath")?;
    let head_revision = git_output(&workspace, ["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    let staged_index = git_output_bytes(&workspace, ["ls-files", "--stage", "-z"])?;
    let index_flags = git_output_bytes(&workspace, ["ls-files", "-v", "-z"])?;
    let index_digest = sha256_bytes(
        format!(
            "workspace-index-v1\0{}\0{}",
            sha256_bytes(&staged_index),
            sha256_bytes(&index_flags)
        )
        .as_bytes(),
    );
    let tracked_paths = parse_tracked_index_paths(&staged_index)?;
    let mut tracked = Vec::with_capacity(tracked_paths.len());
    for (relative, index_mode) in tracked_paths {
        tracked.push(workspace_source_entry(
            &workspace,
            &relative,
            Some(&index_mode),
        )?);
    }

    let untracked_raw = git_output_bytes(
        &workspace,
        ["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    let mut untracked_paths = parse_nul_paths(&untracked_raw, "untracked Git path")?;
    untracked_paths.sort();
    let mut untracked = Vec::with_capacity(untracked_paths.len());
    for relative in untracked_paths {
        untracked.push(workspace_source_entry(&workspace, &relative, None)?);
    }

    let state = WorkspaceSourceState {
        schema_version: 2,
        head_revision,
        index_digest,
        tracked,
        untracked,
    };
    let bytes = serde_json::to_vec(&state).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("cannot serialize Workspace source state: {error}"),
            None,
            false,
        )
    })?;
    Ok(sha256_bytes(&bytes))
}

fn parse_tracked_index_paths(
    staged_index: &[u8],
) -> Result<BTreeMap<String, String>, UniversalExecError> {
    let mut paths = BTreeMap::new();
    for raw in staged_index
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let record = String::from_utf8(raw.to_vec()).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ArtifactNotUtf8,
                format!("tracked Git index record is not UTF-8: {error}"),
                None,
                false,
            )
        })?;
        let (metadata, relative) = record.split_once('\t').ok_or_else(|| {
            UniversalExecError::new(
                UniversalExecErrorCode::MetadataCorrupt,
                "tracked Git index record has no path separator",
                None,
                false,
            )
        })?;
        let index_mode = metadata.split_whitespace().next().ok_or_else(|| {
            UniversalExecError::new(
                UniversalExecErrorCode::MetadataCorrupt,
                "tracked Git index record has no mode",
                None,
                false,
            )
        })?;
        validate_relative_path(relative, "trackedPath")?;
        paths.insert(relative.to_string(), index_mode.to_string());
    }
    Ok(paths)
}

fn parse_nul_paths(bytes: &[u8], label: &str) -> Result<Vec<String>, UniversalExecError> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            String::from_utf8(raw.to_vec()).map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ArtifactNotUtf8,
                    format!("{label} is not UTF-8: {error}"),
                    None,
                    false,
                )
            })
        })
        .collect()
}

fn workspace_source_entry(
    workspace: &Path,
    relative: &str,
    index_mode: Option<&str>,
) -> Result<WorkspaceSourceEntry, UniversalExecError> {
    let relative_path = validate_relative_path(relative, "sourcePath")?;
    let path = workspace.join(&relative_path);
    let before = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkspaceSourceEntry {
                path: relative.to_string(),
                kind: if index_mode == Some("160000") {
                    "gitlink-uninitialized".to_string()
                } else {
                    "missing".to_string()
                },
                mode: 0,
                byte_length: 0,
                digest: sha256_bytes(b"missing"),
            });
        }
        Err(error) => return Err(io_error(&path, "inspect source state", error)),
    };
    let mode = before.permissions().mode() & 0o7777;
    let (kind, byte_length, digest) = if before.file_type().is_symlink() {
        let target =
            fs::read_link(&path).map_err(|error| io_error(&path, "read source symlink", error))?;
        let bytes = target.as_os_str().as_bytes();
        (
            "symlink".to_string(),
            bytes.len() as u64,
            sha256_bytes(bytes),
        )
    } else if before.is_file() {
        ("file".to_string(), before.len(), sha256_file(&path)?)
    } else if before.is_dir() && index_mode == Some("160000") {
        if is_git_worktree(&path)? {
            (
                "git-worktree".to_string(),
                0,
                workspace_source_state_digest_at(&path)?,
            )
        } else {
            (
                "gitlink-uninitialized".to_string(),
                0,
                sha256_bytes(b"gitlink-uninitialized"),
            )
        }
    } else {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspacePathDenied,
            format!("source path is not a regular file, symlink, or Git worktree: {relative}"),
            Some("workspaceId"),
            false,
        ));
    };
    let after = fs::symlink_metadata(&path)
        .map_err(|error| io_error(&path, "reinspect source state", error))?;
    if !same_source_metadata(&before, &after) {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceMutationIncomplete,
            format!("source path changed while its commitment was computed: {relative}"),
            Some("workspaceId"),
            true,
        ));
    }
    Ok(WorkspaceSourceEntry {
        path: relative.to_string(),
        kind,
        mode,
        byte_length,
        digest,
    })
}

fn is_git_worktree(path: &Path) -> Result<bool, UniversalExecError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|error| tool_unavailable("git rev-parse worktree", error))?;
    Ok(output.status.success() && output.stdout == b"true\n")
}

fn same_source_metadata(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.size() == right.size()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

pub fn remove_git_workspace(
    config: &UniversalExecutorConfig,
    request: &WorkspaceCloseRequest,
) -> Result<WorkspaceCloseResult, UniversalExecError> {
    request.validate_shape()?;
    config.ensure_store()?;
    let record_path = config.workspace_record_path(&request.workspace_id);
    let target = config.workspace_path(&request.workspace_id);

    if !record_path.exists() {
        if target.exists() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::MetadataCorrupt,
                "workspace directory exists without an identity record",
                Some("workspaceId"),
                false,
            ));
        }
        return Ok(WorkspaceCloseResult {
            workspace_id: request.workspace_id.clone(),
            removed: false,
        });
    }

    let bytes = read_workspace_record_bytes(&record_path)?;
    if let Some(closed) = decode_closed_workspace_record(&bytes)? {
        validate_closed_identity(&closed, &request.workspace_id)?;
        return Ok(WorkspaceCloseResult {
            workspace_id: request.workspace_id.clone(),
            removed: false,
        });
    }
    let record = decode_open_workspace_record(&bytes, &request.workspace_id)?;

    if !target.exists() {
        if Path::new(&record.workspace_path) != target {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::MetadataCorrupt,
                "workspace record path mismatch",
                Some("workspacePath"),
                false,
            ));
        }
        let final_head = recover_missing_workspace_head(&record)?;
        if let Some(head) = final_head
            .as_deref()
            .filter(|head| *head != record.source_revision)
        {
            let source_repo = Path::new(&record.source_repo);
            if source_repo.is_dir() {
                ensure_rescue_ref(source_repo, &request.workspace_id, head)?;
            }
        }
        write_closed_workspace_record(&record_path, &record, final_head, "already_missing")?;
        return Ok(WorkspaceCloseResult {
            workspace_id: request.workspace_id.clone(),
            removed: false,
        });
    }

    let expected = canonical_directory(&target, "workspacePath")?;
    let recorded = canonical_directory(Path::new(&record.workspace_path), "workspacePath")?;
    if expected != recorded {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            "workspace record path mismatch",
            Some("workspacePath"),
            false,
        ));
    }
    if !request.force {
        let dirty = workspace_dirty_paths(&recorded)?;
        if !dirty.is_empty() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::WorkspaceDirty,
                format!(
                    "workspace contains uncommitted or untracked paths: {}",
                    dirty.join(", ")
                ),
                Some("workspaceId"),
                false,
            ));
        }
    }

    let final_head = git_output(&recorded, ["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    if final_head != record.source_revision {
        ensure_rescue_ref(&recorded, &request.workspace_id, &final_head)?;
    }
    remove_git_worktree_from_workspace(&recorded, request.force)?;
    write_closed_workspace_record(&record_path, &record, Some(final_head), "removed")?;
    Ok(WorkspaceCloseResult {
        workspace_id: request.workspace_id.clone(),
        removed: true,
    })
}

fn write_closed_workspace_record(
    record_path: &Path,
    open: &WorkspaceRecord,
    final_head: Option<String>,
    removal_result: &str,
) -> Result<(), UniversalExecError> {
    let closed = ClosedWorkspaceRecord {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        state: "closed".to_string(),
        workspace_id: open.workspace_id.clone(),
        source_repo: open.source_repo.clone(),
        source_revision: open.source_revision.clone(),
        final_head,
        closed_unix_ms: now_unix_ms()?,
        removal_result: removal_result.to_string(),
    };
    write_json_atomic(record_path, &closed)
}

fn ensure_rescue_ref(
    git_root: &Path,
    workspace_id: &str,
    final_head: &str,
) -> Result<(), UniversalExecError> {
    let reference = format!("refs/ordivon/closed/{workspace_id}");
    let existing = Command::new("git")
        .arg("-C")
        .arg(git_root)
        .args(["rev-parse", "--verify", "--quiet", &reference])
        .output()
        .map_err(|error| tool_unavailable("git rev-parse rescue ref", error))?;
    if existing.status.success() {
        let observed = String::from_utf8(existing.stdout).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                format!("git rescue ref output is not UTF-8: {error}"),
                None,
                false,
            )
        })?;
        if observed.trim() == final_head {
            return Ok(());
        }
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::RevisionMismatch,
            "workspace rescue ref already points to a different commit",
            Some("workspaceId"),
            false,
        ));
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(git_root)
        .args(["update-ref", &reference, final_head])
        .output()
        .map_err(|error| tool_unavailable("git update-ref", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(tool_failed("git update-ref", &output.stderr))
    }
}

fn recover_missing_workspace_head(
    record: &WorkspaceRecord,
) -> Result<Option<String>, UniversalExecError> {
    let source_repo = Path::new(&record.source_repo);
    if !source_repo.is_dir() {
        return Ok(None);
    }
    let rescue_ref = format!("refs/ordivon/closed/{}", record.workspace_id);
    let rescued = Command::new("git")
        .arg("-C")
        .arg(source_repo)
        .args(["rev-parse", "--verify", "--quiet", &rescue_ref])
        .output()
        .map_err(|error| tool_unavailable("git rev-parse rescue ref", error))?;
    if rescued.status.success() {
        return String::from_utf8(rescued.stdout)
            .map(|value| Some(value.trim().to_string()))
            .map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ToolFailed,
                    format!("git rescue ref output is not UTF-8: {error}"),
                    None,
                    false,
                )
            });
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(source_repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|error| tool_unavailable("git worktree list", error))?;
    if !output.status.success() {
        return Err(tool_failed("git worktree list", &output.stderr));
    }
    let wanted = Path::new(&record.workspace_path);
    let text = String::from_utf8(output.stdout).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!("git worktree list output is not UTF-8: {error}"),
            None,
            false,
        )
    })?;
    let mut matched = false;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            matched = Path::new(path) == wanted;
        } else if matched {
            if let Some(head) = line.strip_prefix("HEAD ") {
                return Ok(Some(head.to_string()));
            }
            if line.is_empty() {
                matched = false;
            }
        }
    }
    Ok(None)
}

fn remove_git_worktree_from_workspace(
    workspace: &Path,
    force: bool,
) -> Result<(), UniversalExecError> {
    let common_dir = git_output(
        workspace,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common_dir = PathBuf::from(common_dir.trim());
    let mut command = Command::new("git");
    command
        .arg("--git-dir")
        .arg(common_dir)
        .args(["worktree", "remove"]);
    if force {
        command.arg("--force");
    }
    let output = command
        .arg(workspace)
        .output()
        .map_err(|error| tool_unavailable("git worktree remove", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(tool_failed("git worktree remove", &output.stderr))
    }
}

pub(crate) fn resolve_existing_workspace_path(
    record: &WorkspaceRecord,
    relative: &str,
    allow_directory: bool,
) -> Result<PathBuf, UniversalExecError> {
    let relative = validate_relative_path(relative, "relativePath")?;
    let root = canonical_directory(Path::new(&record.workspace_path), "workspacePath")?;
    let candidate = root.join(&relative);
    let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            UniversalExecError::new(
                UniversalExecErrorCode::WorkspacePathNotFound,
                format!("workspace path does not exist: {}", relative.display()),
                Some("relativePath"),
                false,
            )
        } else {
            io_error(&candidate, "inspect", error)
        }
    })?;
    if metadata.file_type().is_symlink() {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspacePathDenied,
            "workspace path cannot be a symlink",
            Some("relativePath"),
            false,
        ));
    }
    let canonical = fs::canonicalize(&candidate)
        .map_err(|error| io_error(&candidate, "canonicalize", error))?;
    if !canonical.starts_with(&root) || (!allow_directory && canonical.is_dir()) {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspacePathDenied,
            "workspace path escaped its root",
            Some("relativePath"),
            false,
        ));
    }
    Ok(canonical)
}

#[cfg(feature = "transactional-runtime")]
pub(crate) fn resolve_workspace_cwd(
    record: &WorkspaceRecord,
    relative: &str,
) -> Result<PathBuf, UniversalExecError> {
    resolve_existing_workspace_path(record, relative, true).and_then(|path| {
        if !path.is_dir() {
            Err(invalid(
                "cwdRelative must resolve to a directory",
                "cwdRelative",
            ))
        } else {
            Ok(path)
        }
    })
}

pub(crate) fn preflight_workspace_write_path(
    record: &WorkspaceRecord,
    relative: &str,
) -> Result<PathBuf, UniversalExecError> {
    let relative = validate_relative_path(relative, "relativePath")?;
    let root = canonical_directory(Path::new(&record.workspace_path), "workspacePath")?;
    let mut current = root.clone();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            let std::path::Component::Normal(name) = component else {
                continue;
            };
            current.push(name);
            if current.exists() {
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|error| io_error(&current, "inspect", error))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(UniversalExecError::new(
                        UniversalExecErrorCode::WorkspacePathDenied,
                        "workspace parent must remain a non-symlink directory",
                        Some("relativePath"),
                        false,
                    ));
                }
            }
        }
    }
    let target = root.join(relative);
    if target.exists() {
        let metadata =
            fs::symlink_metadata(&target).map_err(|error| io_error(&target, "inspect", error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::WorkspacePathDenied,
                "write target must be a non-symlink regular file",
                Some("relativePath"),
                false,
            ));
        }
    }
    Ok(target)
}

pub(crate) fn remove_workspace_file(
    record: &WorkspaceRecord,
    relative: &str,
) -> Result<(), UniversalExecError> {
    let path = preflight_workspace_write_path(record, relative)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| io_error(&path, "remove", error))?;
    }
    Ok(())
}

fn resolve_workspace_write_path(
    record: &WorkspaceRecord,
    relative: &str,
) -> Result<PathBuf, UniversalExecError> {
    let relative = validate_relative_path(relative, "relativePath")?;
    let root = canonical_directory(Path::new(&record.workspace_path), "workspacePath")?;
    let file_name = relative
        .file_name()
        .ok_or_else(|| invalid("relativePath has no file name", "relativePath"))?;
    let mut safe_parent = root.clone();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            let std::path::Component::Normal(name) = component else {
                continue;
            };
            let next = safe_parent.join(name);
            if next.exists() {
                let metadata = fs::symlink_metadata(&next)
                    .map_err(|error| io_error(&next, "inspect", error))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(UniversalExecError::new(
                        UniversalExecErrorCode::WorkspacePathDenied,
                        "workspace parent must remain a non-symlink directory",
                        Some("relativePath"),
                        false,
                    ));
                }
            } else {
                fs::create_dir(&next).map_err(|error| io_error(&next, "create", error))?;
            }
            safe_parent =
                fs::canonicalize(&next).map_err(|error| io_error(&next, "canonicalize", error))?;
            if !safe_parent.starts_with(&root) {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::WorkspacePathDenied,
                    "workspace write path escaped its root",
                    Some("relativePath"),
                    false,
                ));
            }
        }
    }
    Ok(safe_parent.join(file_name))
}

fn git_output<'a>(
    repo: &Path,
    args: impl IntoIterator<Item = &'a str>,
) -> Result<String, UniversalExecError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| tool_unavailable("git", error))?;
    if !output.status.success() {
        return Err(tool_failed("git", &output.stderr));
    }
    String::from_utf8(output.stdout).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!("git output is not UTF-8: {error}"),
            None,
            false,
        )
    })
}

fn workspace_dirty_paths(workspace: &Path) -> Result<Vec<String>, UniversalExecError> {
    const MAX_DIRTY_PATHS: usize = 20;
    let tracked = git_output_bytes(workspace, ["diff", "--name-only", "-z", "HEAD", "--"])?;
    let untracked = git_output_bytes(
        workspace,
        ["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    let mut paths = Vec::new();
    for raw in tracked
        .split(|byte| *byte == 0)
        .chain(untracked.split(|byte| *byte == 0))
    {
        if raw.is_empty() {
            continue;
        }
        let path = String::from_utf8_lossy(raw).into_owned();
        if !paths.contains(&path) {
            paths.push(path);
        }
        if paths.len() == MAX_DIRTY_PATHS {
            paths.push("…".to_string());
            break;
        }
    }
    Ok(paths)
}

fn git_output_bytes<'a>(
    repo: &Path,
    args: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<u8>, UniversalExecError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| tool_unavailable("git", error))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(tool_failed("git", &output.stderr))
    }
}

fn remove_git_worktree(
    source_repo: &Path,
    workspace: &Path,
    force: bool,
) -> Result<(), UniversalExecError> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(source_repo)
        .args(["worktree", "remove"]);
    if force {
        command.arg("--force");
    }
    let output = command
        .arg(workspace)
        .output()
        .map_err(|error| tool_unavailable("git worktree remove", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(tool_failed("git worktree remove", &output.stderr))
    }
}

fn tool_unavailable(operation: &str, error: impl std::fmt::Display) -> UniversalExecError {
    UniversalExecError::new(
        UniversalExecErrorCode::ToolUnavailable,
        format!("cannot execute {operation}: {error}"),
        None,
        false,
    )
}

fn tool_failed(operation: &str, stderr: &[u8]) -> UniversalExecError {
    let message = String::from_utf8_lossy(stderr).trim().to_string();
    let code =
        if message.contains("No space left on device") || message.contains("Disk quota exceeded") {
            UniversalExecErrorCode::WorkspaceCapacityExceeded
        } else {
            UniversalExecErrorCode::ToolFailed
        };
    UniversalExecError::new(code, format!("{operation} failed: {message}"), None, false)
}

fn transfer_workspace_ownership(root: &Path, uid: u32, gid: u32) -> Result<(), UniversalExecError> {
    fn chown_nofollow(path: &Path, uid: u32, gid: u32) -> Result<(), UniversalExecError> {
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| {
            UniversalExecError::new(
                UniversalExecErrorCode::WorkspacePathDenied,
                "workspace ownership path contains NUL",
                Some("workspacePath"),
                false,
            )
        })?;
        let result = unsafe { libc::lchown(c_path.as_ptr(), uid, gid) };
        if result != 0 {
            return Err(io_error(
                path,
                "change workspace ownership",
                std::io::Error::last_os_error(),
            ));
        }
        Ok(())
    }

    fn visit(path: &Path, uid: u32, gid: u32) -> Result<(), UniversalExecError> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| io_error(path, "inspect ownership target", error))?;
        if metadata.is_dir() {
            chown_nofollow(path, 0, gid)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o770))
                .map_err(|error| io_error(path, "set workspace directory mode", error))?;
            for entry in fs::read_dir(path)
                .map_err(|error| io_error(path, "read ownership directory", error))?
            {
                let entry = entry.map_err(|error| io_error(path, "read ownership entry", error))?;
                visit(&entry.path(), uid, gid)?;
            }
        } else if path.file_name().is_some_and(|name| name == ".git") {
            chown_nofollow(path, 0, 0)?;
            if metadata.is_file() {
                fs::set_permissions(path, fs::Permissions::from_mode(0o400))
                    .map_err(|error| io_error(path, "protect Git worktree identity", error))?;
            }
        } else {
            chown_nofollow(path, uid, gid)?;
        }
        Ok(())
    }

    visit(root, uid, gid)?;
    let metadata =
        fs::metadata(root).map_err(|error| io_error(root, "verify workspace ownership", error))?;
    if metadata.uid() != 0
        || metadata.gid() != gid
        || metadata.permissions().mode() & 0o7777 != 0o770
    {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspacePathDenied,
            "workspace trust-root ownership did not persist",
            Some("workspacePath"),
            false,
        ));
    }
    Ok(())
}
