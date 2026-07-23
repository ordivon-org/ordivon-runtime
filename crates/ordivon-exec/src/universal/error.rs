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
    MetadataCorrupt,
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
