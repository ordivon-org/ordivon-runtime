use schemars::JsonSchema;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecErrorCode {
    InvalidRequest,
    PathNotFound,
    PathNotFile,
    PathNotDirectory,
    IoError,
    UnsupportedEncoding,
    StartLineOutOfRange,
    LineExceedsByteBudget,
    BatchBudgetExhausted,
    ToolUnavailable,
    ToolFailed,
    InvalidToolOutput,
    OutputLimitExceeded,
    RepositoryNotFound,
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecError {
    pub code: ExecErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub retryable: bool,
}

impl ExecError {
    pub(crate) fn new(
        code: ExecErrorCode,
        message: impl Into<String>,
        path: Option<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            path,
            retryable,
        }
    }
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ExecError {}
