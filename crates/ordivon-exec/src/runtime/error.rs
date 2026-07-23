use rusqlite::{Error as SqlError, ErrorCode as SqlErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeErrorCode {
    InvalidRequest,
    RegistryUnavailable,
    RegistryBusy,
    RegistryCorrupt,
    SchemaVersionUnsupported,
    MigrationChecksumMismatch,
    IdempotencyConflict,
    ConcurrencyLimit,
    WorkspaceBusy,
    WorkspaceDirty,
    JobNotFound,
    AttemptNotFound,
    AttemptStateConflict,
    JobAlreadyResolved,
    ResultIdentityConflict,
    DispatchOutcomeUnknown,
    LaunchIdentityMismatch,
    ReservationStateConflict,
    ArtifactIdentityConflict,
    ReconciliationRequired,
    OrphanRemediationDenied,
    IoError,
    ToolUnavailable,
    ToolFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCapacity {
    pub scope: String,
    pub active: u32,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeError {
    pub code: RuntimeErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<Box<RuntimeCapacity>>,
}
impl RuntimeError {
    pub fn new(
        code: RuntimeErrorCode,
        message: impl Into<String>,
        field: Option<&str>,
        retryable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            field: field.map(ToString::to_string),
            retryable,
            retry_after_ms: None,
            capacity: None,
        }
    }

    pub fn concurrency(message: impl Into<String>, field: &str, capacity: RuntimeCapacity) -> Self {
        Self {
            code: RuntimeErrorCode::ConcurrencyLimit,
            message: message.into(),
            field: Some(field.to_string()),
            retryable: true,
            retry_after_ms: Some(1_000),
            capacity: Some(Box::new(capacity)),
        }
    }

    pub fn invalid(message: impl Into<String>, field: &str) -> Self {
        Self::new(
            RuntimeErrorCode::InvalidRequest,
            message,
            Some(field),
            false,
        )
    }

    pub(crate) fn from_sql(error: SqlError, context: &str) -> Self {
        let (code, retryable) = match &error {
            SqlError::SqliteFailure(failure, _) => match failure.code {
                SqlErrorCode::DatabaseBusy | SqlErrorCode::DatabaseLocked => {
                    (RuntimeErrorCode::RegistryBusy, true)
                }
                SqlErrorCode::DatabaseCorrupt | SqlErrorCode::NotADatabase => {
                    (RuntimeErrorCode::RegistryCorrupt, false)
                }
                _ => (RuntimeErrorCode::RegistryUnavailable, false),
            },
            _ => (RuntimeErrorCode::RegistryUnavailable, false),
        };
        Self::new(code, format!("{context}: {error}"), None, retryable)
    }
}

impl Display for RuntimeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for RuntimeError {}

pub type RuntimeResult<T> = Result<T, RuntimeError>;
