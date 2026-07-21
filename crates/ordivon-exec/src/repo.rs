use crate::{ExecError, ExecErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

const MAX_GIT_STATUS_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSnapshotRequest {
    pub root: String,
    pub include_untracked: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSnapshotResult {
    pub root: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u64,
    pub behind: u64,
    pub changed_files: u64,
    pub staged_files: u64,
    pub unstaged_files: u64,
    pub untracked_files: u64,
    pub conflicted_files: u64,
    pub dirty: bool,
}

pub fn repo_snapshot(request: &RepoSnapshotRequest) -> Result<RepoSnapshotResult, ExecError> {
    let root = fs::canonicalize(&request.root).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            ExecErrorCode::PathNotFound
        } else {
            ExecErrorCode::IoError
        };
        ExecError::new(code, error.to_string(), Some(request.root.clone()), false)
    })?;
    if !root.is_dir() {
        return Err(ExecError::new(
            ExecErrorCode::PathNotDirectory,
            "repository root is not a directory",
            Some(root.display().to_string()),
            false,
        ));
    }

    let output = Command::new("git")
        .args([
            "-C",
            root.to_string_lossy().as_ref(),
            "status",
            "--porcelain=v2",
            "--branch",
            if request.include_untracked {
                "--untracked-files=normal"
            } else {
                "--untracked-files=no"
            },
        ])
        .output()
        .map_err(|error| {
            let code = if error.kind() == std::io::ErrorKind::NotFound {
                ExecErrorCode::ToolUnavailable
            } else {
                ExecErrorCode::IoError
            };
            ExecError::new(code, error.to_string(), None, false)
        })?;

    if output.stdout.len() > MAX_GIT_STATUS_BYTES || output.stderr.len() > MAX_GIT_STATUS_BYTES {
        return Err(ExecError::new(
            ExecErrorCode::OutputLimitExceeded,
            format!("git status exceeded {MAX_GIT_STATUS_BYTES} bytes"),
            Some(root.display().to_string()),
            true,
        ));
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let code = if stderr.contains("not a git repository") {
            ExecErrorCode::RepositoryNotFound
        } else {
            ExecErrorCode::ToolFailed
        };
        return Err(ExecError::new(
            code,
            if stderr.is_empty() {
                format!("git status exited with {}", output.status)
            } else {
                stderr
            },
            Some(root.display().to_string()),
            false,
        ));
    }
    let stdout = std::str::from_utf8(&output.stdout).map_err(|error| {
        ExecError::new(
            ExecErrorCode::InvalidToolOutput,
            format!("git status returned non-UTF-8 output: {error}"),
            Some(root.display().to_string()),
            false,
        )
    })?;
    parse_porcelain_v2(root.display().to_string(), stdout)
}

fn parse_porcelain_v2(root: String, output: &str) -> Result<RepoSnapshotResult, ExecError> {
    let mut result = RepoSnapshotResult {
        root,
        ..RepoSnapshotResult::default()
    };
    for line in output.lines() {
        if let Some(value) = line.strip_prefix("# branch.oid ") {
            if value != "(initial)" {
                result.head = Some(value.to_string());
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.head ") {
            if value != "(detached)" {
                result.branch = Some(value.to_string());
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.upstream ") {
            result.upstream = Some(value.to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.ab ") {
            let mut parts = value.split_whitespace();
            result.ahead = parse_distance(parts.next(), '+')?;
            result.behind = parse_distance(parts.next(), '-')?;
            continue;
        }
        if let Some(status) = line.strip_prefix("1 ").or_else(|| line.strip_prefix("2 ")) {
            let xy = status.split_whitespace().next().unwrap_or("..");
            count_xy(&mut result, xy);
            result.changed_files += 1;
            continue;
        }
        if line.starts_with("u ") {
            result.conflicted_files += 1;
            result.changed_files += 1;
            continue;
        }
        if line.starts_with("? ") {
            result.untracked_files += 1;
            result.changed_files += 1;
        }
    }
    result.dirty = result.changed_files > 0;
    Ok(result)
}

fn parse_distance(value: Option<&str>, prefix: char) -> Result<u64, ExecError> {
    value
        .and_then(|item| item.strip_prefix(prefix))
        .ok_or_else(|| invalid_git("invalid branch.ab record"))?
        .parse::<u64>()
        .map_err(|_| invalid_git("invalid branch distance"))
}

fn count_xy(result: &mut RepoSnapshotResult, xy: &str) {
    let mut chars = xy.chars();
    if chars.next().is_some_and(|value| value != '.') {
        result.staged_files += 1;
    }
    if chars.next().is_some_and(|value| value != '.') {
        result.unstaged_files += 1;
    }
}

fn invalid_git(message: &str) -> ExecError {
    ExecError::new(
        ExecErrorCode::InvalidToolOutput,
        format!("invalid git porcelain v2 output: {message}"),
        None,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_snapshot_from_one_git_status_call() {
        let output = "# branch.oid abc123\n# branch.head feature/x\n# branch.upstream origin/feature/x\n# branch.ab +2 -1\n1 M. N... 100644 100644 100644 a b src/a.rs\n1 .M N... 100644 100644 100644 a b src/b.rs\n? src/new.rs\nu UU N... 100644 100644 100644 100644 a b c src/conflict.rs\n";
        let result = parse_porcelain_v2("/repo".to_string(), output).unwrap();
        assert_eq!(result.head.as_deref(), Some("abc123"));
        assert_eq!(result.ahead, 2);
        assert_eq!(result.behind, 1);
        assert_eq!(result.changed_files, 4);
        assert_eq!(result.staged_files, 1);
        assert_eq!(result.unstaged_files, 1);
        assert_eq!(result.untracked_files, 1);
        assert_eq!(result.conflicted_files, 1);
        assert!(result.dirty);
    }
}
