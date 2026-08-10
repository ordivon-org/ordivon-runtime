use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UniversalExecErrorCode {
    InvalidRequest,
    WorkspaceExists,
    WorkspaceNotFound,
    WorkspacePathNotFound,
    WorkspaceDirty,
    WorkspacePathDenied,
    RevisionNotFound,
    RevisionMismatch,
    WorkspaceStateMismatch,
    InputStateMismatch,
    HostDependencyRuntimeDrift,
    ExecutableRuntimeDrift,
    WorkspaceMutationIncomplete,
    TaskExists,
    TaskNotFound,
    TaskStartFailed,
    TaskStateUnavailable,
    ArtifactNotFound,
    ArtifactNotUtf8,
    OutputLimitExceeded,
    ToolUnavailable,
    ToolFailed,
    IoError,
    WorkspaceCapacityExceeded,
    MetadataCorrupt,
}

impl UniversalExecErrorCode {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::WorkspaceExists => "WORKSPACE_EXISTS",
            Self::WorkspaceNotFound => "WORKSPACE_NOT_FOUND",
            Self::WorkspacePathNotFound => "WORKSPACE_PATH_NOT_FOUND",
            Self::WorkspaceDirty => "WORKSPACE_DIRTY",
            Self::WorkspacePathDenied => "WORKSPACE_PATH_DENIED",
            Self::RevisionNotFound => "REVISION_NOT_FOUND",
            Self::RevisionMismatch => "REVISION_MISMATCH",
            Self::WorkspaceStateMismatch => "WORKSPACE_STATE_MISMATCH",
            Self::InputStateMismatch => "INPUT_STATE_MISMATCH",
            Self::HostDependencyRuntimeDrift => "HOST_DEPENDENCY_RUNTIME_DRIFT",
            Self::ExecutableRuntimeDrift => "EXECUTABLE_RUNTIME_DRIFT",
            Self::WorkspaceMutationIncomplete => "WORKSPACE_MUTATION_INCOMPLETE",
            Self::TaskExists => "TASK_EXISTS",
            Self::TaskNotFound => "TASK_NOT_FOUND",
            Self::TaskStartFailed => "TASK_START_FAILED",
            Self::TaskStateUnavailable => "TASK_STATE_UNAVAILABLE",
            Self::ArtifactNotFound => "ARTIFACT_NOT_FOUND",
            Self::ArtifactNotUtf8 => "ARTIFACT_NOT_UTF8",
            Self::OutputLimitExceeded => "OUTPUT_LIMIT_EXCEEDED",
            Self::ToolUnavailable => "TOOL_UNAVAILABLE",
            Self::ToolFailed => "TOOL_FAILED",
            Self::IoError => "IO_ERROR",
            Self::WorkspaceCapacityExceeded => "WORKSPACE_CAPACITY_EXCEEDED",
            Self::MetadataCorrupt => "METADATA_CORRUPT",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UniversalExecError {
    pub code: UniversalExecErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
}

impl UniversalExecError {
    pub fn new_for_cli(message: impl Into<String>) -> Self {
        Self::new(UniversalExecErrorCode::InvalidRequest, message, None, false)
    }

    pub(crate) fn new(
        code: UniversalExecErrorCode,
        message: impl Into<String>,
        field: Option<&str>,
        retryable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            field: field.map(ToString::to_string),
            retryable,
        }
    }
}

impl std::fmt::Display for UniversalExecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for UniversalExecError {}
