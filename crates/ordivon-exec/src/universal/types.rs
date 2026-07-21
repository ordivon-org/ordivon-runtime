use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{
    invalid, validate_args, validate_artifact_id, validate_env, validate_id,
    validate_relative_path, UniversalExecError, MAX_ARTIFACT_READ_BYTES, MAX_TASK_WAIT_MS,
    MAX_UNIVERSAL_OUTPUT_BYTES, MAX_UNIVERSAL_RUNTIME_MS, MAX_WORKSPACE_IO_BYTES,
    UNIVERSAL_EXEC_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitWorkspaceCreateRequest {
    pub schema_version: u32,
    pub workspace_id: String,
    pub source_repo: String,
    pub source_revision: String,
}

impl GitWorkspaceCreateRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.workspace_id, "workspaceId")?;
        if self.source_repo.is_empty()
            || !std::path::Path::new(&self.source_repo).is_absolute()
            || self.source_repo.as_bytes().contains(&0)
        {
            return Err(invalid(
                "sourceRepo must be an absolute NUL-free path",
                "sourceRepo",
            ));
        }
        if self.source_revision.trim().is_empty()
            || self.source_revision.len() > 256
            || self.source_revision.as_bytes().contains(&0)
        {
            return Err(invalid(
                "sourceRevision must be non-empty, bounded, and NUL-free",
                "sourceRevision",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceRecord {
    pub schema_version: u32,
    pub workspace_id: String,
    pub source_repo: String,
    pub source_revision: String,
    pub workspace_path: String,
    pub created_unix_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceReadRequest {
    pub schema_version: u32,
    pub workspace_id: String,
    pub relative_path: String,
    pub max_bytes: u64,
}

impl WorkspaceReadRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.workspace_id, "workspaceId")?;
        validate_relative_path(&self.relative_path, "relativePath")?;
        if self.max_bytes == 0 || self.max_bytes > MAX_WORKSPACE_IO_BYTES {
            return Err(invalid(
                format!("maxBytes must be in 1..={MAX_WORKSPACE_IO_BYTES}"),
                "maxBytes",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceReadResult {
    pub workspace_id: String,
    pub relative_path: String,
    pub content: String,
    pub digest: String,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceWriteRequest {
    pub schema_version: u32,
    pub workspace_id: String,
    pub relative_path: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_digest: Option<String>,
}

impl WorkspaceWriteRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.workspace_id, "workspaceId")?;
        validate_relative_path(&self.relative_path, "relativePath")?;
        if self.content.len() as u64 > MAX_WORKSPACE_IO_BYTES {
            return Err(invalid(
                format!("content exceeds {MAX_WORKSPACE_IO_BYTES} bytes"),
                "content",
            ));
        }
        if self
            .expected_digest
            .as_ref()
            .is_some_and(|digest| !valid_digest(digest))
        {
            return Err(invalid("expectedDigest must be SHA-256", "expectedDigest"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceWriteResult {
    pub workspace_id: String,
    pub relative_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_digest: Option<String>,
    pub after_digest: String,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceDiffRequest {
    pub schema_version: u32,
    pub workspace_id: String,
    pub max_bytes: u64,
}

impl WorkspaceDiffRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.workspace_id, "workspaceId")?;
        if self.max_bytes == 0 || self.max_bytes > MAX_WORKSPACE_IO_BYTES {
            return Err(invalid(
                format!("maxBytes must be in 1..={MAX_WORKSPACE_IO_BYTES}"),
                "maxBytes",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceDiffResult {
    pub workspace_id: String,
    pub diff: String,
    pub digest: String,
    pub byte_length: u64,
    pub truncated: bool,
    pub untracked_paths: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UniversalExecRequest {
    pub schema_version: u32,
    pub task_id: String,
    pub workspace_id: String,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd_relative: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
}

impl UniversalExecRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.task_id, "taskId")?;
        validate_id(&self.workspace_id, "workspaceId")?;
        if self.executable.is_empty()
            || !std::path::Path::new(&self.executable).is_absolute()
            || self.executable.as_bytes().contains(&0)
        {
            return Err(invalid(
                "executable must be an absolute NUL-free path",
                "executable",
            ));
        }
        validate_args(&self.args)?;
        validate_relative_path(&self.cwd_relative, "cwdRelative")?;
        validate_env(&self.env)?;
        if self.timeout_ms == 0 || self.timeout_ms > MAX_UNIVERSAL_RUNTIME_MS {
            return Err(invalid(
                format!("timeoutMs must be in 1..={MAX_UNIVERSAL_RUNTIME_MS}"),
                "timeoutMs",
            ));
        }
        for (value, field) in [
            (self.stdout_limit_bytes, "stdoutLimitBytes"),
            (self.stderr_limit_bytes, "stderrLimitBytes"),
        ] {
            if value == 0 || value > MAX_UNIVERSAL_OUTPUT_BYTES {
                return Err(invalid(
                    format!("{field} must be in 1..={MAX_UNIVERSAL_OUTPUT_BYTES}"),
                    field,
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskGetRequest {
    pub schema_version: u32,
    pub task_id: String,
    #[serde(default)]
    pub wait_ms: u64,
}

impl TaskGetRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.task_id, "taskId")?;
        if self.wait_ms > MAX_TASK_WAIT_MS {
            return Err(invalid(
                format!("waitMs must be in 0..={MAX_TASK_WAIT_MS}"),
                "waitMs",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskCancelRequest {
    pub schema_version: u32,
    pub task_id: String,
}

impl TaskCancelRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.task_id, "taskId")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactReadRequest {
    pub schema_version: u32,
    pub task_id: String,
    pub artifact_id: String,
    pub offset: u64,
    pub max_bytes: u64,
}

impl ArtifactReadRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.task_id, "taskId")?;
        validate_artifact_id(&self.artifact_id, "artifactId")?;
        if self.max_bytes == 0 || self.max_bytes > MAX_ARTIFACT_READ_BYTES {
            return Err(invalid(
                format!("maxBytes must be in 1..={MAX_ARTIFACT_READ_BYTES}"),
                "maxBytes",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactReadResult {
    pub task_id: String,
    pub artifact_id: String,
    pub content: String,
    pub offset: u64,
    pub next_offset: u64,
    pub eof: bool,
    pub digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RunnerTaskRequest {
    pub schema_version: u32,
    pub task_id: String,
    pub workspace_id: String,
    pub workspace_path: String,
    pub executable: String,
    pub executable_digest: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TaskMetadata {
    pub schema_version: u32,
    pub task_id: String,
    pub workspace_id: String,
    pub unit_name: String,
    pub request_digest: String,
    pub boot_id: String,
    pub created_unix_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum TaskTerminalStatus {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapturedOutput {
    pub artifact_id: String,
    pub file_name: String,
    pub digest: String,
    pub retained_bytes: u64,
    pub dropped_bytes: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RunnerTaskResult {
    pub schema_version: u32,
    pub task_id: String,
    pub status: TaskTerminalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infrastructure_error: Option<String>,
    pub started_unix_ms: u128,
    pub finished_unix_ms: u128,
    pub stdout: CapturedOutput,
    pub stderr: CapturedOutput,
}

fn require_schema(version: u32) -> Result<(), UniversalExecError> {
    if version != UNIVERSAL_EXEC_SCHEMA_VERSION {
        return Err(invalid(
            "unsupported universal executor schema version",
            "schemaVersion",
        ));
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}
