use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};

use super::{
    canonical_directory, invalid, io_error, now_unix_ms, open_directory_nofollow,
    open_regular_file_beneath, sha256_bytes, sha256_file, validate_relative_path,
    write_bytes_atomic, write_json_atomic, GitWorkspaceCreateRequest, UniversalExecError,
    UniversalExecErrorCode, UniversalExecutorConfig, WorkspaceChangeCursor, WorkspaceChangeEntry,
    WorkspaceChangeKind, WorkspaceChangePageRequest, WorkspaceChangePageResult,
    WorkspaceCloseRequest, WorkspaceCloseResult, WorkspaceClosureDisposition,
    WorkspaceContentMetadata, WorkspaceContentReadResult, WorkspaceContentRequest,
    WorkspaceDiffRequest, WorkspaceDiffResult, WorkspaceReadRequest, WorkspaceReadResult,
    WorkspaceRecord, WorkspaceRenamedPath, WorkspaceWriteRequest, WorkspaceWriteResult,
    UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClosedWorkspaceRecord {
    schema_version: u32,
    state: String,
    workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_state_digest: Option<String>,
    closed_unix_ms: u128,
    removal_result: String,
}

#[derive(Debug, Default)]
pub(crate) struct WorkspaceChangeProjection {
    pub(crate) changed: Vec<String>,
    pub(crate) modified: Vec<String>,
    pub(crate) added: Vec<String>,
    pub(crate) deleted: Vec<String>,
    pub(crate) renamed: Vec<WorkspaceRenamedPath>,
    pub(crate) untracked: Vec<String>,
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
    let revision = resolve_git_commit(&source_repo, &request.source_revision)?;
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

#[derive(Debug)]
pub(crate) struct WorkspaceRecordInventoryIssue {
    pub workspace_id: String,
    pub error: UniversalExecError,
}

#[derive(Debug)]
pub(crate) struct WorkspaceRecordInventory {
    pub records: Vec<WorkspaceRecord>,
    pub issues: Vec<WorkspaceRecordInventoryIssue>,
}

pub(crate) fn list_workspace_record_inventory(
    config: &UniversalExecutorConfig,
) -> Result<WorkspaceRecordInventory, UniversalExecError> {
    config.ensure_store()?;
    let records_root = config.workspace_records_root();
    let mut records = Vec::new();
    let mut issues = Vec::new();
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
        let record = match load_workspace_record_metadata(config, workspace_id) {
            Ok(record) => record,
            Err(error) if error.code == UniversalExecErrorCode::WorkspaceNotFound => continue,
            Err(error) => {
                issues.push(WorkspaceRecordInventoryIssue {
                    workspace_id: workspace_id.to_string(),
                    error,
                });
                continue;
            }
        };
        let expected_path = config.workspace_path(workspace_id);
        if Path::new(&record.workspace_path) != expected_path {
            issues.push(WorkspaceRecordInventoryIssue {
                workspace_id: workspace_id.to_string(),
                error: UniversalExecError::new(
                    UniversalExecErrorCode::MetadataCorrupt,
                    "workspace record path does not match its identity",
                    Some("workspaceId"),
                    false,
                ),
            });
            continue;
        }
        match fs::symlink_metadata(&expected_path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                records.push(record);
            }
            Ok(_) => issues.push(WorkspaceRecordInventoryIssue {
                workspace_id: workspace_id.to_string(),
                error: UniversalExecError::new(
                    UniversalExecErrorCode::MetadataCorrupt,
                    "workspace record target must be a non-symlink directory",
                    Some("workspaceId"),
                    false,
                ),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => issues.push(WorkspaceRecordInventoryIssue {
                workspace_id: workspace_id.to_string(),
                error: io_error(&expected_path, "inspect", error),
            }),
        }
    }
    records.sort_by(|left, right| {
        right
            .created_unix_ms
            .cmp(&left.created_unix_ms)
            .then_with(|| left.workspace_id.cmp(&right.workspace_id))
    });
    issues.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
    Ok(WorkspaceRecordInventory { records, issues })
}

pub(crate) fn list_open_workspace_record_inventory(
    config: &UniversalExecutorConfig,
) -> Result<WorkspaceRecordInventory, UniversalExecError> {
    let workspaces_root = config.workspaces_root();
    let mut records = Vec::new();
    let mut issues = Vec::new();
    for entry in
        fs::read_dir(&workspaces_root).map_err(|error| io_error(&workspaces_root, "list", error))?
    {
        let entry =
            entry.map_err(|error| io_error(&workspaces_root, "read directory entry", error))?;
        let workspace_id = entry.file_name().to_string_lossy().into_owned();
        if let Err(error) = super::validate_id(&workspace_id, "workspaceId") {
            issues.push(WorkspaceRecordInventoryIssue {
                workspace_id,
                error,
            });
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| io_error(&entry.path(), "inspect", error))?;
        if !file_type.is_dir() || file_type.is_symlink() {
            issues.push(WorkspaceRecordInventoryIssue {
                workspace_id,
                error: UniversalExecError::new(
                    UniversalExecErrorCode::MetadataCorrupt,
                    "workspace target must be a non-symlink directory",
                    Some("workspaceId"),
                    false,
                ),
            });
            continue;
        }
        let record = match load_workspace_record_metadata(config, &workspace_id) {
            Ok(record) => record,
            Err(error) => {
                issues.push(WorkspaceRecordInventoryIssue {
                    workspace_id,
                    error,
                });
                continue;
            }
        };
        let expected_path = config.workspace_path(&workspace_id);
        if Path::new(&record.workspace_path) != expected_path {
            issues.push(WorkspaceRecordInventoryIssue {
                workspace_id,
                error: UniversalExecError::new(
                    UniversalExecErrorCode::MetadataCorrupt,
                    "workspace record path does not match its identity",
                    Some("workspaceId"),
                    false,
                ),
            });
            continue;
        }
        records.push(record);
    }
    records.sort_by(|left, right| {
        right
            .created_unix_ms
            .cmp(&left.created_unix_ms)
            .then_with(|| left.workspace_id.cmp(&right.workspace_id))
    });
    issues.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
    Ok(WorkspaceRecordInventory { records, issues })
}

pub(crate) fn workspace_cleanup_dependents(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
) -> Result<Vec<String>, UniversalExecError> {
    super::validate_id(workspace_id, "workspaceId")?;
    config.ensure_store()?;
    let destructive_roots = [
        config.workspace_path(workspace_id),
        config.workspace_cache_path(workspace_id),
        config.workspace_build_cache_path(workspace_id),
        config.workspace_tmp_path(workspace_id),
    ];
    let records_root = config.workspace_records_root();
    let mut dependents = Vec::new();
    for entry in
        fs::read_dir(&records_root).map_err(|error| io_error(&records_root, "list", error))?
    {
        let entry =
            entry.map_err(|error| io_error(&records_root, "read directory entry", error))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(candidate_id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if candidate_id == workspace_id {
            continue;
        }
        let record = match load_workspace_record_metadata(config, candidate_id) {
            Ok(record) => record,
            Err(error) if error.code == UniversalExecErrorCode::WorkspaceNotFound => continue,
            Err(error) => return Err(error),
        };
        let workspace_path = Path::new(&record.workspace_path);
        let authority = workspace_git_common_dir_at(workspace_path)
            .unwrap_or_else(|_| PathBuf::from(&record.source_repo));
        if destructive_roots
            .iter()
            .any(|root| authority.starts_with(root))
        {
            dependents.push(record.workspace_id);
        }
    }
    dependents.sort();
    dependents.dedup();
    Ok(dependents)
}

pub fn list_workspace_records(
    config: &UniversalExecutorConfig,
    limit: u32,
) -> Result<Vec<WorkspaceRecord>, UniversalExecError> {
    if limit == 0 {
        return Err(invalid("limit must be positive", "limit"));
    }
    let mut inventory = list_workspace_record_inventory(config)?;
    inventory.records.truncate(limit as usize);
    if let Some(issue) = inventory.issues.into_iter().next() {
        return Err(issue.error);
    }
    Ok(inventory.records)
}

fn open_workspace_regular_file(
    record: &WorkspaceRecord,
    relative: &str,
) -> Result<(File, PathBuf), UniversalExecError> {
    let relative_path = validate_relative_path(relative, "relativePath")?;
    let workspace_root = Path::new(&record.workspace_path);
    let logical_path = workspace_root.join(&relative_path);
    let root = open_directory_nofollow(workspace_root).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::WorkspacePathDenied,
            format!("cannot open Workspace root without following symlinks: {error}"),
            Some("workspaceId"),
            false,
        )
    })?;
    let file = open_regular_file_beneath(&root, &relative_path, false).map_err(|error| {
        let code = match error.raw_os_error() {
            Some(libc::ENOENT) => UniversalExecErrorCode::WorkspacePathNotFound,
            Some(libc::ELOOP) | Some(libc::EXDEV) | Some(libc::ENOTDIR) => {
                UniversalExecErrorCode::WorkspacePathDenied
            }
            Some(libc::ENOSYS) => UniversalExecErrorCode::ToolUnavailable,
            _ if error.kind() == std::io::ErrorKind::InvalidInput => {
                UniversalExecErrorCode::WorkspacePathDenied
            }
            _ => UniversalExecErrorCode::IoError,
        };
        UniversalExecError::new(
            code,
            format!("cannot open Workspace file beneath its root: {error}"),
            Some("relativePath"),
            false,
        )
    })?;
    Ok((file, logical_path))
}

fn read_workspace_file_bounded(
    mut file: File,
    logical_path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, UniversalExecError> {
    let metadata = file
        .metadata()
        .map_err(|error| io_error(logical_path, "inspect opened file", error))?;
    if metadata.len() > max_bytes {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::OutputLimitExceeded,
            format!("file exceeds maxBytes {max_bytes}"),
            Some("maxBytes"),
            false,
        ));
    }
    let mut bytes = Vec::with_capacity((metadata.len().min(max_bytes) + 1) as usize);
    file.by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(logical_path, "read opened file", error))?;
    if bytes.len() as u64 > max_bytes {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::OutputLimitExceeded,
            format!("file exceeds maxBytes {max_bytes}"),
            Some("maxBytes"),
            false,
        ));
    }
    Ok(bytes)
}

pub fn read_workspace_text(
    config: &UniversalExecutorConfig,
    request: &WorkspaceReadRequest,
) -> Result<WorkspaceReadResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let (file, logical_path) = open_workspace_regular_file(&record, &request.relative_path)?;
    let bytes = read_workspace_file_bounded(file, &logical_path, request.max_bytes)?;
    let digest = sha256_bytes(&bytes);
    let byte_length = bytes.len() as u64;
    let content = String::from_utf8(bytes).map_err(|error| {
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
        digest,
        byte_length,
    })
}

fn verified_workspace_image_media_type(
    relative_path: &str,
    bytes: &[u8],
) -> Result<&'static str, UniversalExecError> {
    let extension = Path::new(relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("png") if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Ok("image/png"),
        Some("jpg" | "jpeg") if bytes.starts_with(&[0xff, 0xd8, 0xff]) => Ok("image/jpeg"),
        Some("png" | "jpg" | "jpeg") => Err(UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            "workspace image bytes do not match the file extension",
            Some("relativePath"),
            false,
        )),
        _ => Err(UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            "workspace.content currently supports only verified .png, .jpg, and .jpeg images",
            Some("relativePath"),
            false,
        )),
    }
}

pub fn read_workspace_content(
    config: &UniversalExecutorConfig,
    request: &WorkspaceContentRequest,
) -> Result<WorkspaceContentReadResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let (file, logical_path) = open_workspace_regular_file(&record, &request.relative_path)?;
    let bytes = read_workspace_file_bounded(file, &logical_path, request.max_bytes)?;
    let digest = sha256_bytes(&bytes);
    if digest != request.expected_digest {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::RevisionMismatch,
            format!(
                "workspace content digest changed: expected {}, observed {digest}",
                request.expected_digest
            ),
            Some("expectedDigest"),
            false,
        ));
    }
    let media_type = verified_workspace_image_media_type(&request.relative_path, &bytes)?;
    Ok(WorkspaceContentReadResult {
        metadata: WorkspaceContentMetadata {
            workspace_id: request.workspace_id.clone(),
            relative_path: request.relative_path.clone(),
            digest,
            media_type: media_type.to_string(),
            byte_length: bytes.len() as u64,
        },
        bytes,
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

const MAX_GIT_DIAGNOSTIC_BYTES: usize = 64 * 1024;

fn drain_reader_bounded<R>(mut reader: R) -> JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut retained = Vec::with_capacity(MAX_GIT_DIAGNOSTIC_BYTES);
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            let remaining = MAX_GIT_DIAGNOSTIC_BYTES.saturating_sub(retained.len());
            retained.extend_from_slice(&buffer[..read.min(remaining)]);
        }
        retained
    })
}

fn finish_stream_child(
    mut child: Child,
    stderr: JoinHandle<Vec<u8>>,
    context: &str,
    intentionally_stopped: bool,
) -> Result<(), UniversalExecError> {
    if intentionally_stopped {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("wait for {context}: {error}"),
            None,
            true,
        )
    })?;
    let stderr = stderr.join().unwrap_or_default();
    if !intentionally_stopped && !status.success() {
        return Err(tool_failed(context, &stderr));
    }
    Ok(())
}

fn bounded_command_stdout(
    command: &mut Command,
    max_bytes: u64,
    allowed_exit_codes: &[i32],
    context: &str,
) -> Result<(Vec<u8>, bool), UniversalExecError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| tool_unavailable(context, error))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!("{context} stdout pipe is unavailable"),
            None,
            false,
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!("{context} stderr pipe is unavailable"),
            None,
            false,
        )
    })?;
    let stderr = drain_reader_bounded(stderr);
    let mut bytes = Vec::with_capacity(
        usize::try_from(
            max_bytes
                .min(super::MAX_WORKSPACE_IO_BYTES)
                .saturating_add(1),
        )
        .unwrap_or(0),
    );
    let read_result = stdout
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes);
    if let Err(error) = read_result {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stderr.join();
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("read {context} output: {error}"),
            None,
            true,
        ));
    }
    let truncated = bytes.len() as u64 > max_bytes;
    if truncated {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("wait for {context}: {error}"),
            None,
            true,
        )
    })?;
    let stderr = stderr.join().unwrap_or_default();
    if !truncated {
        let code = status.code().unwrap_or(-1);
        if !allowed_exit_codes.contains(&code) {
            return Err(tool_failed(context, &stderr));
        }
    }
    bytes.truncate(usize::try_from(max_bytes).unwrap_or(usize::MAX));
    Ok((bytes, truncated))
}

fn bounded_utf8(
    mut bytes: Vec<u8>,
    truncated: bool,
    context: &str,
) -> Result<(String, Vec<u8>), UniversalExecError> {
    if let Err(error) = std::str::from_utf8(&bytes) {
        if truncated && error.error_len().is_none() {
            bytes.truncate(error.valid_up_to());
        } else {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::ArtifactNotUtf8,
                format!("{context} is not UTF-8: {error}"),
                None,
                false,
            ));
        }
    }
    let text = String::from_utf8(bytes.clone()).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            format!("{context} is not UTF-8: {error}"),
            None,
            false,
        )
    })?;
    Ok((text, bytes))
}

pub fn workspace_diff(
    config: &UniversalExecutorConfig,
    request: &WorkspaceDiffRequest,
) -> Result<WorkspaceDiffResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let workspace = Path::new(&record.workspace_path);
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(workspace)
        .args(["diff", "HEAD", "--no-ext-diff", "--no-color", "--binary"]);
    let (bytes, truncated) =
        bounded_command_stdout(&mut command, request.max_bytes, &[0], "git diff")?;
    let (diff, bytes) = bounded_utf8(bytes, truncated, "git diff output")?;
    let changes = workspace_change_projection_at(workspace)?;
    Ok(WorkspaceDiffResult {
        workspace_id: request.workspace_id.clone(),
        diff,
        digest: sha256_bytes(&bytes),
        byte_length: bytes.len() as u64,
        truncated,
        changed_paths: changes.changed,
        modified_paths: changes.modified,
        added_paths: changes.added,
        deleted_paths: changes.deleted,
        renamed_paths: changes.renamed,
        untracked_paths: changes.untracked,
    })
}

fn workspace_changed_paths(
    workspace: &Path,
) -> Result<WorkspaceChangeProjection, UniversalExecError> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(workspace)
        .args([
            "diff",
            "HEAD",
            "--name-status",
            "-z",
            "--find-renames",
            "--find-copies",
        ])
        .output()
        .map_err(|error| tool_unavailable("git diff --name-status", error))?;
    if !output.status.success() {
        return Err(tool_failed("git diff --name-status", &output.stderr));
    }
    let fields: Vec<&[u8]> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect();
    let mut changed = BTreeSet::new();
    let mut modified = BTreeSet::new();
    let mut added = BTreeSet::new();
    let mut deleted = BTreeSet::new();
    let mut renamed = Vec::new();
    let mut index = 0usize;
    while index < fields.len() {
        let status = std::str::from_utf8(fields[index]).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ArtifactNotUtf8,
                format!("git diff status is not UTF-8: {error}"),
                None,
                false,
            )
        })?;
        index += 1;
        let code = status.as_bytes().first().copied().ok_or_else(|| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                "git diff emitted an empty path status",
                None,
                false,
            )
        })?;
        let path = |raw: &[u8]| -> Result<String, UniversalExecError> {
            String::from_utf8(raw.to_vec()).map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ArtifactNotUtf8,
                    format!("changed Git path is not UTF-8: {error}"),
                    None,
                    false,
                )
            })
        };
        match code {
            b'R' | b'C' => {
                if index + 1 >= fields.len() {
                    return Err(UniversalExecError::new(
                        UniversalExecErrorCode::ToolFailed,
                        "git diff rename/copy record is incomplete",
                        None,
                        false,
                    ));
                }
                let from_path = path(fields[index])?;
                let to_path = path(fields[index + 1])?;
                index += 2;
                if code == b'R' {
                    changed.insert(from_path.clone());
                    changed.insert(to_path.clone());
                    renamed.push(WorkspaceRenamedPath { from_path, to_path });
                } else {
                    changed.insert(to_path.clone());
                    added.insert(to_path);
                }
            }
            b'M' | b'T' | b'U' => {
                if index >= fields.len() {
                    return Err(UniversalExecError::new(
                        UniversalExecErrorCode::ToolFailed,
                        "git diff path record is incomplete",
                        None,
                        false,
                    ));
                }
                let value = path(fields[index])?;
                index += 1;
                changed.insert(value.clone());
                modified.insert(value);
            }
            b'A' => {
                if index >= fields.len() {
                    return Err(UniversalExecError::new(
                        UniversalExecErrorCode::ToolFailed,
                        "git diff added-path record is incomplete",
                        None,
                        false,
                    ));
                }
                let value = path(fields[index])?;
                index += 1;
                changed.insert(value.clone());
                added.insert(value);
            }
            b'D' => {
                if index >= fields.len() {
                    return Err(UniversalExecError::new(
                        UniversalExecErrorCode::ToolFailed,
                        "git diff deleted-path record is incomplete",
                        None,
                        false,
                    ));
                }
                let value = path(fields[index])?;
                index += 1;
                changed.insert(value.clone());
                deleted.insert(value);
            }
            _ => {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::ToolFailed,
                    format!("unsupported git diff path status: {status}"),
                    None,
                    false,
                ));
            }
        }
    }
    Ok(WorkspaceChangeProjection {
        changed: changed.into_iter().collect(),
        modified: modified.into_iter().collect(),
        added: added.into_iter().collect(),
        deleted: deleted.into_iter().collect(),
        renamed,
        untracked: Vec::new(),
    })
}

pub(crate) fn workspace_change_projection_at(
    workspace: &Path,
) -> Result<WorkspaceChangeProjection, UniversalExecError> {
    let workspace = canonical_directory(workspace, "workspacePath")?;
    let mut changes = workspace_changed_paths(&workspace)?;
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(&workspace)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .output()
        .map_err(|error| tool_unavailable("git ls-files", error))?;
    if !output.status.success() {
        return Err(tool_failed("git ls-files", &output.stderr));
    }
    for raw in output.stdout.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        changes
            .untracked
            .push(String::from_utf8(raw.to_vec()).map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ArtifactNotUtf8,
                    format!("untracked Git path is not UTF-8: {error}"),
                    None,
                    false,
                )
            })?);
    }
    Ok(changes)
}

fn read_nul_field<R: BufRead>(
    reader: &mut R,
    context: &str,
) -> Result<Option<Vec<u8>>, UniversalExecError> {
    let max_bytes = usize::try_from(super::MAX_WORKSPACE_IO_BYTES).unwrap_or(usize::MAX);
    let mut field = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("read {context}: {error}"),
                None,
                true,
            )
        })?;
        if available.is_empty() {
            if field.is_empty() {
                return Ok(None);
            }
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                format!("{context} ended before NUL terminator"),
                None,
                false,
            ));
        }
        if let Some(index) = available.iter().position(|byte| *byte == 0) {
            if field.len().saturating_add(index) > max_bytes {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::OutputLimitExceeded,
                    format!("{context} field exceeds {max_bytes} bytes"),
                    Some("maxBytes"),
                    false,
                ));
            }
            field.extend_from_slice(&available[..index]);
            reader.consume(index + 1);
            return Ok(Some(field));
        }
        if field.len().saturating_add(available.len()) > max_bytes {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::OutputLimitExceeded,
                format!("{context} field exceeds {max_bytes} bytes"),
                Some("maxBytes"),
                false,
            ));
        }
        let consumed = available.len();
        field.extend_from_slice(available);
        reader.consume(consumed);
    }
}

fn utf8_change_path(raw: Vec<u8>, context: &str) -> Result<String, UniversalExecError> {
    String::from_utf8(raw).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            format!("{context} is not UTF-8: {error}"),
            None,
            false,
        )
    })
}

fn tracked_change_entry<R: BufRead>(
    reader: &mut R,
) -> Result<Option<WorkspaceChangeEntry>, UniversalExecError> {
    let Some(status) = read_nul_field(reader, "git diff --name-status")? else {
        return Ok(None);
    };
    let status = std::str::from_utf8(&status).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            format!("git diff status is not UTF-8: {error}"),
            None,
            false,
        )
    })?;
    let code = status.as_bytes().first().copied().ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            "git diff emitted an empty path status",
            None,
            false,
        )
    })?;
    let one_path = |reader: &mut R, kind: WorkspaceChangeKind| {
        let raw = read_nul_field(reader, "git diff path")?.ok_or_else(|| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                "git diff path record is incomplete",
                None,
                false,
            )
        })?;
        Ok(WorkspaceChangeEntry {
            kind,
            path: utf8_change_path(raw, "changed Git path")?,
        })
    };
    match code {
        b'M' | b'T' | b'U' => one_path(reader, WorkspaceChangeKind::Modified).map(Some),
        b'A' => one_path(reader, WorkspaceChangeKind::Added).map(Some),
        b'D' => one_path(reader, WorkspaceChangeKind::Deleted).map(Some),
        _ => Err(UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!("unsupported git diff path status: {status}"),
            None,
            false,
        )),
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ChangeOrderKey {
    path: String,
    kind: WorkspaceChangeKind,
}

impl ChangeOrderKey {
    fn from_entry(entry: &WorkspaceChangeEntry) -> Self {
        Self {
            path: entry.path.clone(),
            kind: entry.kind,
        }
    }

    fn from_cursor(cursor: &WorkspaceChangeCursor) -> Self {
        Self {
            path: cursor.after_path.clone(),
            kind: cursor.after_kind,
        }
    }
}

#[derive(Debug)]
struct ChangeCandidate {
    order_key: ChangeOrderKey,
    encoded: Vec<u8>,
    entry: WorkspaceChangeEntry,
}

impl PartialEq for ChangeCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.order_key == other.order_key && self.encoded == other.encoded
    }
}

impl Eq for ChangeCandidate {}

impl PartialOrd for ChangeCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChangeCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.order_key
            .cmp(&other.order_key)
            .then_with(|| self.encoded.cmp(&other.encoded))
    }
}

#[derive(Default)]
struct ChangeSetAccumulator {
    xor: [u8; 32],
    sums: [u64; 4],
    count: u64,
}

impl ChangeSetAccumulator {
    fn observe(&mut self, encoded: &[u8]) {
        let digest = Sha256::digest(encoded);
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&digest);
        for (target, value) in self.xor.iter_mut().zip(bytes.iter()) {
            *target ^= *value;
        }
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            let mut word = [0_u8; 8];
            word.copy_from_slice(chunk);
            self.sums[index] = self.sums[index].wrapping_add(u64::from_be_bytes(word));
        }
        self.count = self.count.saturating_add(1);
    }

    fn digest(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"ordivon-workspace-change-set-v1\0");
        digest.update(self.count.to_be_bytes());
        digest.update(self.xor);
        for sum in self.sums {
            digest.update(sum.to_be_bytes());
        }
        format!("sha256:{}", hex::encode(digest.finalize()))
    }
}

struct ChangePageCollector {
    after_key: Option<ChangeOrderKey>,
    after_key_seen: bool,
    limit: usize,
    max_bytes: u64,
    candidates: BinaryHeap<ChangeCandidate>,
    candidate_bytes: u64,
    eligible_count: u64,
    smallest_oversized_key: Option<ChangeOrderKey>,
    accumulator: ChangeSetAccumulator,
}

impl ChangePageCollector {
    fn new(request: &WorkspaceChangePageRequest) -> Result<Self, UniversalExecError> {
        let after_key = request.cursor.as_ref().map(ChangeOrderKey::from_cursor);
        Ok(Self {
            after_key_seen: after_key.is_none(),
            after_key,
            limit: request.limit as usize,
            max_bytes: request.max_bytes,
            candidates: BinaryHeap::new(),
            candidate_bytes: 0,
            eligible_count: 0,
            smallest_oversized_key: None,
            accumulator: ChangeSetAccumulator::default(),
        })
    }

    fn observe(&mut self, entry: WorkspaceChangeEntry) -> Result<(), UniversalExecError> {
        let encoded = serde_json::to_vec(&entry).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                format!("cannot encode workspace change entry: {error}"),
                None,
                false,
            )
        })?;
        self.accumulator.observe(&encoded);
        let order_key = ChangeOrderKey::from_entry(&entry);
        if self.after_key.as_ref() == Some(&order_key) {
            self.after_key_seen = true;
        }
        if self
            .after_key
            .as_ref()
            .is_some_and(|after| &order_key <= after)
        {
            return Ok(());
        }
        self.eligible_count = self.eligible_count.saturating_add(1);
        if encoded.len() as u64 > self.max_bytes {
            if self
                .smallest_oversized_key
                .as_ref()
                .is_none_or(|current| &order_key < current)
            {
                self.smallest_oversized_key = Some(order_key);
            }
            return Ok(());
        }
        self.candidate_bytes = self.candidate_bytes.saturating_add(encoded.len() as u64);
        self.candidates.push(ChangeCandidate {
            order_key,
            encoded,
            entry,
        });
        let memory_budget = self.max_bytes.saturating_mul(2);
        while self.candidates.len() > self.limit.saturating_add(1)
            || (self.candidate_bytes > memory_budget && self.candidates.len() > 1)
        {
            if let Some(removed) = self.candidates.pop() {
                self.candidate_bytes = self
                    .candidate_bytes
                    .saturating_sub(removed.encoded.len() as u64);
            }
        }
        Ok(())
    }

    fn finish(
        self,
        cursor: Option<&WorkspaceChangeCursor>,
    ) -> Result<WorkspaceChangePageSelection, UniversalExecError> {
        let total_entries = self.accumulator.count;
        let change_set_digest = self.accumulator.digest();
        if let Some(cursor) = cursor {
            if cursor.change_set_digest != change_set_digest {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::WorkspaceStateMismatch,
                    "workspace change set changed since the previous page",
                    Some("cursor.changeSetDigest"),
                    false,
                ));
            }
            if !self.after_key_seen {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::WorkspaceStateMismatch,
                    "cursor afterPath/afterKind is not present in the current change set",
                    Some("cursor.afterPath"),
                    false,
                ));
            }
        }
        let mut candidates = self.candidates.into_vec();
        candidates.sort();
        let oversized_boundary = self.smallest_oversized_key;
        let mut entries = Vec::with_capacity(self.limit);
        let mut entry_bytes = 0_u64;
        let mut last_key = None;
        for candidate in candidates {
            if oversized_boundary
                .as_ref()
                .is_some_and(|boundary| &candidate.order_key > boundary)
            {
                break;
            }
            let cost = candidate.encoded.len() as u64 + u64::from(!entries.is_empty());
            if entries.len() >= self.limit || entry_bytes.saturating_add(cost) > self.max_bytes {
                break;
            }
            entry_bytes = entry_bytes.saturating_add(cost);
            last_key = Some(candidate.order_key);
            entries.push(candidate.entry);
        }
        if entries.is_empty() && self.eligible_count > 0 {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::OutputLimitExceeded,
                "the next workspace change entry exceeds maxBytes",
                Some("maxBytes"),
                false,
            ));
        }
        let remaining_entries = self.eligible_count.saturating_sub(entries.len() as u64);
        let complete = remaining_entries == 0;
        let next_cursor = if complete {
            None
        } else {
            let last_key = last_key.ok_or_else(|| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ToolFailed,
                    "change page could not establish a continuation key",
                    None,
                    false,
                )
            })?;
            Some(WorkspaceChangeCursor {
                change_set_digest: change_set_digest.clone(),
                after_path: last_key.path,
                after_kind: last_key.kind,
            })
        };
        Ok(WorkspaceChangePageSelection {
            change_set_digest,
            entries,
            entry_bytes,
            total_entries,
            remaining_entries,
            complete,
            next_cursor,
        })
    }
}

struct WorkspaceChangePageSelection {
    change_set_digest: String,
    entries: Vec<WorkspaceChangeEntry>,
    entry_bytes: u64,
    total_entries: u64,
    remaining_entries: u64,
    complete: bool,
    next_cursor: Option<WorkspaceChangeCursor>,
}

fn scan_tracked_changes(
    workspace: &Path,
    collector: &mut ChangePageCollector,
) -> Result<(), UniversalExecError> {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(workspace)
        .args([
            "diff",
            "HEAD",
            "--name-status",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| tool_unavailable("git diff --name-status", error))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            "git diff --name-status stdout pipe is unavailable",
            None,
            false,
        )
    })?;
    let stderr = drain_reader_bounded(child.stderr.take().ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            "git diff --name-status stderr pipe is unavailable",
            None,
            false,
        )
    })?);
    let mut reader = BufReader::new(stdout);
    loop {
        let candidate = match tracked_change_entry(&mut reader) {
            Ok(candidate) => candidate,
            Err(error) => {
                let _ = finish_stream_child(child, stderr, "git diff --name-status", true);
                return Err(error);
            }
        };
        let Some(candidate) = candidate else {
            finish_stream_child(child, stderr, "git diff --name-status", false)?;
            return Ok(());
        };
        if let Err(error) = collector.observe(candidate) {
            let _ = finish_stream_child(child, stderr, "git diff --name-status", true);
            return Err(error);
        }
    }
}

fn scan_untracked_changes(
    workspace: &Path,
    collector: &mut ChangePageCollector,
) -> Result<(), UniversalExecError> {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(workspace)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| tool_unavailable("git ls-files", error))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            "git ls-files stdout pipe is unavailable",
            None,
            false,
        )
    })?;
    let stderr = drain_reader_bounded(child.stderr.take().ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            "git ls-files stderr pipe is unavailable",
            None,
            false,
        )
    })?);
    let mut reader = BufReader::new(stdout);
    loop {
        let raw = match read_nul_field(&mut reader, "git ls-files") {
            Ok(raw) => raw,
            Err(error) => {
                let _ = finish_stream_child(child, stderr, "git ls-files", true);
                return Err(error);
            }
        };
        let Some(raw) = raw else {
            finish_stream_child(child, stderr, "git ls-files", false)?;
            return Ok(());
        };
        let entry = WorkspaceChangeEntry {
            kind: WorkspaceChangeKind::Untracked,
            path: utf8_change_path(raw, "untracked Git path")?,
        };
        if let Err(error) = collector.observe(entry) {
            let _ = finish_stream_child(child, stderr, "git ls-files", true);
            return Err(error);
        }
    }
}

pub fn workspace_changes_page(
    config: &UniversalExecutorConfig,
    request: &WorkspaceChangePageRequest,
) -> Result<WorkspaceChangePageResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let workspace = Path::new(&record.workspace_path);
    let mut collector = ChangePageCollector::new(request)?;
    scan_tracked_changes(workspace, &mut collector)?;
    scan_untracked_changes(workspace, &mut collector)?;
    let selection = collector.finish(request.cursor.as_ref())?;

    // A page is one structured change-set observation even though tracked and
    // untracked facts come from separate Git processes. Re-scan only path/kind
    // facts after selection so a concurrent Workspace transition cannot silently
    // splice two realities into one page. This intentionally does not hash file
    // contents; byte-only changes that preserve the same change membership/kind
    // remain valid for this projection.
    let mut verification = ChangePageCollector::new(request)?;
    scan_tracked_changes(workspace, &mut verification)?;
    scan_untracked_changes(workspace, &mut verification)?;
    let verification_digest = verification.accumulator.digest();
    if verification_digest != selection.change_set_digest {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceMutationIncomplete,
            "workspace change set changed while projecting the page",
            Some("workspaceId"),
            true,
        ));
    }

    Ok(WorkspaceChangePageResult {
        workspace_id: request.workspace_id.clone(),
        change_set_digest: selection.change_set_digest,
        entries: selection.entries,
        entry_bytes: selection.entry_bytes,
        total_entries: selection.total_entries,
        remaining_entries: selection.remaining_entries,
        complete: selection.complete,
        next_cursor: selection.next_cursor,
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
    let mut truncated = false;
    for relative_path in relative_paths {
        let tracked = Command::new("git")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(workspace)
            .args(["ls-files", "--error-unmatch", "--"])
            .arg(relative_path)
            .output()
            .map_err(|error| tool_unavailable("git ls-files", error))?;
        let remaining = max_bytes.saturating_sub(combined.len() as u64);
        let (bytes, was_truncated) = if tracked.status.success() {
            let mut command = Command::new("git");
            command
                .arg("--no-optional-locks")
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
                .arg(relative_path);
            bounded_command_stdout(&mut command, remaining, &[0], "git diff")?
        } else {
            let mut command = Command::new("git");
            command
                .arg("--no-optional-locks")
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
                .arg(relative_path);
            bounded_command_stdout(&mut command, remaining, &[0, 1], "git diff --no-index")?
        };
        combined.extend_from_slice(&bytes);
        if was_truncated {
            truncated = true;
            break;
        }
    }
    let (diff, _) = bounded_utf8(combined, truncated, "git diff output")?;
    Ok((diff, truncated))
}

pub(crate) fn workspace_head_and_dirty_at(
    workspace: &Path,
) -> Result<(String, bool), UniversalExecError> {
    let workspace = canonical_directory(workspace, "workspacePath")?;
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(&workspace)
        .args([
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ])
        .output()
        .map_err(|error| tool_unavailable("git status", error))?;
    if !output.status.success() {
        return Err(tool_failed("git status", &output.stderr));
    }
    let mut head_revision = None;
    let mut dirty = false;
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        if let Some(value) = raw.strip_prefix(b"# branch.oid ") {
            let value = String::from_utf8(value.to_vec()).map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ArtifactNotUtf8,
                    format!("Git branch OID is not UTF-8: {error}"),
                    None,
                    false,
                )
            })?;
            head_revision = Some(value);
        } else if !raw.starts_with(b"# ") {
            dirty = true;
        }
    }
    let head_revision = head_revision.ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::RevisionNotFound,
            "git status omitted branch.oid",
            Some("workspaceId"),
            false,
        )
    })?;
    if head_revision.len() != 40 && head_revision.len() != 64 {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::RevisionNotFound,
            "workspace HEAD did not resolve to a commit",
            Some("workspaceId"),
            false,
        ));
    }
    Ok((head_revision, dirty))
}

#[cfg(any(feature = "transactional-runtime", test))]
pub fn workspace_is_dirty(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
) -> Result<bool, UniversalExecError> {
    let record = load_workspace_record(config, workspace_id)?;
    workspace_is_dirty_at(Path::new(&record.workspace_path))
}

fn workspace_is_dirty_at(workspace: &Path) -> Result<bool, UniversalExecError> {
    let workspace = canonical_directory(workspace, "workspacePath")?;
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(workspace)
        .args([
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ])
        .output()
        .map_err(|error| tool_unavailable("git status", error))?;
    if !output.status.success() {
        return Err(tool_failed("git status", &output.stderr));
    }
    Ok(!output.stdout.is_empty())
}

#[cfg(any(feature = "transactional-runtime", test))]
pub fn workspace_head_revision(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
) -> Result<String, UniversalExecError> {
    let record = load_workspace_record(config, workspace_id)?;
    workspace_head_revision_at(Path::new(&record.workspace_path))
}

pub(crate) fn workspace_head_revision_at(workspace: &Path) -> Result<String, UniversalExecError> {
    let workspace = canonical_directory(workspace, "workspacePath")?;
    let revision = git_output(&workspace, ["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    if revision.len() != 40 && revision.len() != 64 {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::RevisionNotFound,
            "workspace HEAD did not resolve to a commit",
            Some("workspaceId"),
            false,
        ));
    }
    Ok(revision)
}

pub fn workspace_source_state_digest(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
) -> Result<String, UniversalExecError> {
    let record = load_workspace_record(config, workspace_id)?;
    workspace_source_state_digest_at(Path::new(&record.workspace_path))
}

pub(crate) fn workspace_git_common_dir_at(workspace: &Path) -> Result<PathBuf, UniversalExecError> {
    let workspace = canonical_directory(workspace, "workspacePath")?;
    let common_dir = git_output(
        &workspace,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    canonical_directory(Path::new(common_dir.trim()), "workspaceGitCommonDir")
}

pub(crate) fn workspace_source_state_digest_at(
    workspace: &Path,
) -> Result<String, UniversalExecError> {
    let workspace = canonical_directory(workspace, "workspacePath")?;
    let head_revision = workspace_head_revision_at(&workspace)?;
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
        cleanup_workspace_caches(config, &request.workspace_id)?;
        if request.expected_source_state_digest.is_some() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::RevisionMismatch,
                "cannot prove the requested Workspace source state after identity loss",
                Some("expectedSourceStateDigest"),
                false,
            ));
        }
        return Ok(WorkspaceCloseResult {
            workspace_id: request.workspace_id.clone(),
            removed: false,
            closure_disposition: WorkspaceClosureDisposition::AlreadyAbsent,
            source_state_digest: None,
        });
    }

    let bytes = read_workspace_record_bytes(&record_path)?;
    if let Some(closed) = decode_closed_workspace_record(&bytes)? {
        validate_closed_identity(&closed, &request.workspace_id)?;
        if let Some(expected) = &request.expected_source_state_digest {
            if closed.source_state_digest.as_deref() != Some(expected.as_str()) {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::RevisionMismatch,
                    "closed Workspace source state differs from expectedSourceStateDigest",
                    Some("expectedSourceStateDigest"),
                    false,
                ));
            }
        }
        cleanup_workspace_caches(config, &request.workspace_id)?;
        return Ok(WorkspaceCloseResult {
            workspace_id: request.workspace_id.clone(),
            removed: false,
            closure_disposition: WorkspaceClosureDisposition::AlreadyClosed,
            source_state_digest: closed.source_state_digest,
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
        if request.expected_source_state_digest.is_some() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::RevisionMismatch,
                "cannot prove expectedSourceStateDigest after Workspace directory loss",
                Some("expectedSourceStateDigest"),
                false,
            ));
        }
        cleanup_workspace_caches(config, &request.workspace_id)?;
        write_closed_workspace_record(&record_path, &record, final_head, None, "already_missing")?;
        return Ok(WorkspaceCloseResult {
            workspace_id: request.workspace_id.clone(),
            removed: false,
            closure_disposition: WorkspaceClosureDisposition::RecoveredMissing,
            source_state_digest: None,
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
    let source_state_digest = workspace_source_state_digest_at(&recorded)?;
    if let Some(expected) = &request.expected_source_state_digest {
        if expected != &source_state_digest {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::RevisionMismatch,
                "Workspace source state differs from expectedSourceStateDigest",
                Some("expectedSourceStateDigest"),
                false,
            ));
        }
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
    cleanup_workspace_caches(config, &request.workspace_id)?;
    remove_git_worktree_from_workspace(&recorded, request.force)?;
    write_closed_workspace_record(
        &record_path,
        &record,
        Some(final_head),
        Some(source_state_digest.clone()),
        "removed",
    )?;
    Ok(WorkspaceCloseResult {
        workspace_id: request.workspace_id.clone(),
        removed: true,
        closure_disposition: WorkspaceClosureDisposition::Removed,
        source_state_digest: Some(source_state_digest),
    })
}

fn cleanup_workspace_caches(
    config: &UniversalExecutorConfig,
    workspace_id: &str,
) -> Result<(), UniversalExecError> {
    let tmp_backing = config.workspace_tmp_path(workspace_id);
    let canonical_tmp_backing = config.canonical_workspace_tmp_path(workspace_id)?;
    let tmp_presentation = config.workspace_tmp_presentation_path(workspace_id)?;
    match fs::symlink_metadata(&tmp_presentation) {
        Ok(metadata) => {
            if !metadata.file_type().is_symlink() {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::MetadataCorrupt,
                    format!(
                        "Workspace temporary presentation {} is not a symlink",
                        tmp_presentation.display()
                    ),
                    Some("workspaceId"),
                    false,
                ));
            }
            let target = fs::read_link(&tmp_presentation).map_err(|error| {
                io_error(
                    &tmp_presentation,
                    "read Workspace temporary presentation",
                    error,
                )
            })?;
            if target != canonical_tmp_backing {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::MetadataCorrupt,
                    format!(
                        "Workspace temporary presentation {} points at {}, expected {}",
                        tmp_presentation.display(),
                        target.display(),
                        canonical_tmp_backing.display()
                    ),
                    Some("workspaceId"),
                    false,
                ));
            }
            fs::remove_file(&tmp_presentation).map_err(|error| {
                io_error(
                    &tmp_presentation,
                    "remove Workspace temporary presentation",
                    error,
                )
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(io_error(
                &tmp_presentation,
                "inspect Workspace temporary presentation",
                error,
            ));
        }
    }
    for path in [
        config.workspace_cache_path(workspace_id),
        config.workspace_build_cache_path(workspace_id),
        tmp_backing,
    ] {
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&path, "remove Workspace cache", error)),
        }
    }
    Ok(())
}

fn write_closed_workspace_record(
    record_path: &Path,
    open: &WorkspaceRecord,
    final_head: Option<String>,
    source_state_digest: Option<String>,
    removal_result: &str,
) -> Result<(), UniversalExecError> {
    let closed = ClosedWorkspaceRecord {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        state: "closed".to_string(),
        workspace_id: open.workspace_id.clone(),
        source_repo: Some(open.source_repo.clone()),
        source_revision: Some(open.source_revision.clone()),
        final_head,
        source_state_digest,
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
    field: &str,
) -> Result<PathBuf, UniversalExecError> {
    let path = resolve_existing_workspace_path(record, relative, true).map_err(|mut error| {
        if error.field.as_deref() == Some("relativePath") {
            error.field = Some(field.to_string());
        }
        error
    })?;
    if !path.is_dir() {
        return Err(invalid("cwdRelative must resolve to a directory", field));
    }
    Ok(path)
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

fn resolve_git_commit(repo: &Path, source_revision: &str) -> Result<String, UniversalExecError> {
    let revision_spec = format!("{source_revision}^{{commit}}");
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--end-of-options"])
        .arg(&revision_spec)
        .output()
        .map_err(|error| tool_unavailable("git", error))?;
    if !output.status.success() {
        let repository_probe = Command::new("git")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "--git-dir"])
            .output()
            .map_err(|error| tool_unavailable("git", error))?;
        if !repository_probe.status.success() {
            let message = String::from_utf8_lossy(&repository_probe.stderr)
                .trim()
                .to_string();
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                format!("source repository is not usable by git: {message}"),
                Some("sourceRepo"),
                false,
            ));
        }
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::RevisionNotFound,
            "source revision does not resolve to a commit",
            Some("sourceRevision"),
            false,
        ));
    }
    String::from_utf8(output.stdout)
        .map(|revision| revision.trim().to_string())
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                format!("git revision output is not UTF-8: {error}"),
                Some("sourceRevision"),
                false,
            )
        })
}

fn git_output<'a>(
    repo: &Path,
    args: impl IntoIterator<Item = &'a str>,
) -> Result<String, UniversalExecError> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
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
        .arg("--no-optional-locks")
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

#[cfg(test)]
mod bounded_output_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn bounded_command_stdout_stops_after_one_byte_beyond_budget() {
        let mut command = Command::new("/usr/bin/python3");
        command.args([
            "-c",
            "import sys; sys.stdout.write('x' * 5000000); sys.stdout.flush()",
        ]);
        let started = Instant::now();
        let (bytes, truncated) =
            bounded_command_stdout(&mut command, 64, &[0], "large test output").unwrap();
        assert!(truncated);
        assert_eq!(bytes.len(), 64);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
