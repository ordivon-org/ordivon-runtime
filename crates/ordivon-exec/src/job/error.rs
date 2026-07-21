use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobContractErrorCode {
    InvalidRequest,
    PolicyInvalid,
    ProfileNotFound,
    ProfileDisabled,
    ArgumentPolicyDenied,
    PathScopeDenied,
    InvalidCwd,
    EnvironmentDenied,
    TimeoutExceedsPolicy,
    OutputLimitExceedsPolicy,
    ConcurrencyLimit,
    JobNotFound,
    JobAlreadyTerminal,
    JobStateConflict,
    SupervisorUnavailable,
    RunnerStartFailed,
    JobMetadataCorrupt,
    CursorInvalid,
    OutputNotRetained,
    IdempotencyConflict,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobContractError {
    pub code: JobContractErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
}

impl JobContractError {
    pub(super) fn new(
        code: JobContractErrorCode,
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

impl std::fmt::Display for JobContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for JobContractError {}
