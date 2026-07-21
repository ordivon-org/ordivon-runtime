use rusqlite::{Error as SqlError, ErrorCode as SqlErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum M6ErrorCode {
    InvalidRequest,
    RegistryUnavailable,
    RegistryBusy,
    RegistryCorrupt,
    SchemaVersionUnsupported,
    MigrationChecksumMismatch,
    IdempotencyConflict,
    ConcurrencyLimit,
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
    IoError,
    ToolUnavailable,
    ToolFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M6Error {
    pub code: M6ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
}
impl M6Error {
    pub fn new(
        code: M6ErrorCode,
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

    pub fn invalid(message: impl Into<String>, field: &str) -> Self {
        Self::new(M6ErrorCode::InvalidRequest, message, Some(field), false)
    }

    pub(crate) fn from_sql(error: SqlError, context: &str) -> Self {
        let (code, retryable) = match &error {
            SqlError::SqliteFailure(failure, _) => match failure.code {
                SqlErrorCode::DatabaseBusy | SqlErrorCode::DatabaseLocked => {
                    (M6ErrorCode::RegistryBusy, true)
                }
                SqlErrorCode::DatabaseCorrupt | SqlErrorCode::NotADatabase => {
                    (M6ErrorCode::RegistryCorrupt, false)
                }
                _ => (M6ErrorCode::RegistryUnavailable, false),
            },
            _ => (M6ErrorCode::RegistryUnavailable, false),
        };
        Self::new(code, format!("{context}: {error}"), None, retryable)
    }
}

impl Display for M6Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for M6Error {}

pub type M6Result<T> = Result<T, M6Error>;
