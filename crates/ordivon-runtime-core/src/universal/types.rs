use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    invalid, validate_id, validate_relative_path, UniversalExecError, MAX_WORKSPACE_IO_BYTES,
    MAX_WORKSPACE_MUTATIONS, UNIVERSAL_EXEC_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitWorkspaceCreateRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
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
pub struct CompactWorkspaceOpenResult {
    pub workspace_id: String,
    pub source_revision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceCloseRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub workspace_id: String,
    #[serde(default)]
    pub force: bool,
}

impl WorkspaceCloseRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.workspace_id, "workspaceId")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceCloseResult {
    pub workspace_id: String,
    pub removed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceReadRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub workspace_id: String,
    pub relative_path: String,
    #[schemars(range(min = 1, max = MAX_WORKSPACE_IO_BYTES))]
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
pub struct CompactWorkspaceReadResult {
    pub content: String,
    pub digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceWriteRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
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
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub workspace_id: String,
    #[schemars(range(min = 1, max = MAX_WORKSPACE_IO_BYTES))]
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
pub struct CompactWorkspaceDiffResult {
    pub diff: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    pub untracked_paths: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[schemars(inline)]
pub enum WorkspaceMutationMode {
    Write,
    Append,
    ReplaceExact,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceMutation {
    pub relative_path: String,
    pub mode: WorkspaceMutationMode,
    #[serde(default)]
    pub content: String,
    /// Required when the target already exists; protects the complete file version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_digest: Option<String>,
    /// Required only for REPLACE_EXACT; must occur exactly once in the current file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_text: Option<String>,
}

impl WorkspaceMutation {
    fn validate_shape(&self, mutation_index: usize) -> Result<(), UniversalExecError> {
        validate_relative_path(
            &self.relative_path,
            &format!("mutations[{mutation_index}].relativePath"),
        )?;
        if self.content.len() as u64 > MAX_WORKSPACE_IO_BYTES {
            return Err(invalid(
                format!("mutation content exceeds {MAX_WORKSPACE_IO_BYTES} bytes"),
                format!("mutations[{mutation_index}].content"),
            ));
        }
        if self
            .expected_digest
            .as_ref()
            .is_some_and(|digest| !valid_digest(digest))
        {
            return Err(invalid(
                "expectedDigest must be SHA-256",
                format!("mutations[{mutation_index}].expectedDigest"),
            ));
        }
        match self.mode {
            WorkspaceMutationMode::ReplaceExact => {
                let expected = self.expected_text.as_ref().ok_or_else(|| {
                    invalid(
                        "REPLACE_EXACT requires expectedText",
                        format!("mutations[{mutation_index}].expectedText"),
                    )
                })?;
                if expected.is_empty() || expected.len() as u64 > MAX_WORKSPACE_IO_BYTES {
                    return Err(invalid(
                        "expectedText must be non-empty and bounded",
                        format!("mutations[{mutation_index}].expectedText"),
                    ));
                }
            }
            WorkspaceMutationMode::Write | WorkspaceMutationMode::Append => {
                if self.expected_text.is_some() {
                    return Err(invalid(
                        "expectedText is only valid for REPLACE_EXACT",
                        format!("mutations[{mutation_index}].expectedText"),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceMutateRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub workspace_id: String,
    #[schemars(length(min = 1, max = MAX_WORKSPACE_MUTATIONS))]
    pub mutations: Vec<WorkspaceMutation>,
}

impl WorkspaceMutateRequest {
    pub fn validate_shape(&self) -> Result<(), UniversalExecError> {
        require_schema(self.schema_version)?;
        validate_id(&self.workspace_id, "workspaceId")?;
        if self.mutations.is_empty() || self.mutations.len() > MAX_WORKSPACE_MUTATIONS {
            return Err(invalid(
                format!("mutations must contain 1..={MAX_WORKSPACE_MUTATIONS} items"),
                "mutations",
            ));
        }
        let mut paths = BTreeSet::new();
        for (mutation_index, mutation) in self.mutations.iter().enumerate() {
            mutation.validate_shape(mutation_index)?;
            if !paths.insert(&mutation.relative_path) {
                return Err(invalid(
                    "a batch cannot mutate the same path more than once",
                    format!("mutations[{mutation_index}].relativePath"),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceMutationResult {
    pub relative_path: String,
    pub after_digest: String,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceMutateResult {
    pub mutations: Vec<WorkspaceMutationResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceReadSliceRequest {
    #[schemars(range(min = 1, max = 1), extend("const" = 1))]
    pub schema_version: u32,
    pub workspace_id: String,
    pub relative_path: String,
    #[serde(default)]
    pub offset: u64,
    #[schemars(range(min = 1, max = MAX_WORKSPACE_IO_BYTES))]
    pub max_bytes: u64,
}

impl WorkspaceReadSliceRequest {
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
pub struct WorkspaceReadSliceResult {
    pub workspace_id: String,
    pub relative_path: String,
    pub content: String,
    pub offset: u64,
    pub next_offset: u64,
    pub eof: bool,
    pub file_digest: String,
    pub file_byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactWorkspaceSliceResult {
    pub content: String,
    pub file_digest: String,
    pub file_byte_length: u64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub eof: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RunnerPayloadConfig {
    pub uid: u32,
    pub gid: u32,
    pub workspace_view: String,
    pub cwd_view: String,
    pub runtime_view: String,
    pub cache_view: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RunnerTaskRequest {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<RunnerPayloadConfig>,
    #[serde(default)]
    pub inherit_host_environment: bool,
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
pub(crate) struct RunnerStartEvidence {
    pub schema_version: u32,
    pub job_id: String,
    pub attempt_id: String,
    pub launch_token_digest: String,
    pub unit_name: String,
    pub invocation_id: String,
    pub control_group: String,
    pub namespace_pid: u32,
    pub namespace_process_start_identity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_uid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_gid: Option<u32>,
    pub observed_unix_ms: u128,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_token_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_uid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_gid: Option<u32>,
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
