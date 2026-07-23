use std::fs;
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
    super::validate_id(workspace_id, "workspaceId")?;
    let path = config.workspace_record_path(workspace_id);
    let bytes = fs::read(&path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceNotFound,
            format!("cannot read workspace record: {error}"),
            Some("workspaceId"),
            false,
        )
    })?;
    let record: WorkspaceRecord = serde_json::from_slice(&bytes).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("invalid workspace record: {error}"),
            Some("workspaceId"),
            false,
        )
    })?;
    if record.workspace_id != workspace_id {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            "workspace record identity mismatch",
            Some("workspaceId"),
            false,
        ));
    }
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
        .args(["diff", "--no-ext-diff", "--no-color", "--binary"])
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

pub fn remove_git_workspace(
    config: &UniversalExecutorConfig,
    request: &WorkspaceCloseRequest,
) -> Result<WorkspaceCloseResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    if !request.force {
        let dirty = workspace_dirty_paths(Path::new(&record.workspace_path))?;
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
    remove_git_worktree(
        Path::new(&record.source_repo),
        Path::new(&record.workspace_path),
        request.force,
    )?;
    let record_path = config.workspace_record_path(&request.workspace_id);
    if record_path.exists() {
        fs::remove_file(&record_path).map_err(|error| io_error(&record_path, "remove", error))?;
    }
    Ok(WorkspaceCloseResult {
        workspace_id: request.workspace_id.clone(),
        removed: true,
    })
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
    UniversalExecError::new(
        UniversalExecErrorCode::ToolFailed,
        format!(
            "{operation} failed: {}",
            String::from_utf8_lossy(stderr).trim()
        ),
        None,
        false,
    )
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
