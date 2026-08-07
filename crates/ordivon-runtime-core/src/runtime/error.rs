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
    WorkspaceExists,
    WorkspaceNotFound,
    WorkspacePathNotFound,
    WorkspaceDirty,
    WorkspacePathDenied,
    RevisionNotFound,
    RevisionMismatch,
    WorkspaceStateMismatch,
    MetadataCorrupt,
    JobNotFound,
    AttemptNotFound,
    AttemptStateConflict,
    JobAlreadyResolved,
    ResultIdentityConflict,
    DispatchOutcomeUnknown,
    LaunchIdentityMismatch,
    ReservationStateConflict,
    ArtifactIdentityConflict,
    ArtifactNotFound,
    ArtifactNotUtf8,
    OutputLimitExceeded,
    ReconciliationRequired,
    OrphanRemediationDenied,
    WorkspaceCapacityExceeded,
    IoError,
    ToolUnavailable,
    ToolFailed,
}

impl RuntimeErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::RegistryUnavailable => "REGISTRY_UNAVAILABLE",
            Self::RegistryBusy => "REGISTRY_BUSY",
            Self::RegistryCorrupt => "REGISTRY_CORRUPT",
            Self::SchemaVersionUnsupported => "SCHEMA_VERSION_UNSUPPORTED",
            Self::MigrationChecksumMismatch => "MIGRATION_CHECKSUM_MISMATCH",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::ConcurrencyLimit => "CONCURRENCY_LIMIT",
            Self::WorkspaceBusy => "WORKSPACE_BUSY",
            Self::WorkspaceExists => "WORKSPACE_EXISTS",
            Self::WorkspaceNotFound => "WORKSPACE_NOT_FOUND",
            Self::WorkspacePathNotFound => "WORKSPACE_PATH_NOT_FOUND",
            Self::WorkspaceDirty => "WORKSPACE_DIRTY",
            Self::WorkspacePathDenied => "WORKSPACE_PATH_DENIED",
            Self::RevisionNotFound => "REVISION_NOT_FOUND",
            Self::RevisionMismatch => "REVISION_MISMATCH",
            Self::WorkspaceStateMismatch => "WORKSPACE_STATE_MISMATCH",
            Self::MetadataCorrupt => "METADATA_CORRUPT",
            Self::JobNotFound => "JOB_NOT_FOUND",
            Self::AttemptNotFound => "ATTEMPT_NOT_FOUND",
            Self::AttemptStateConflict => "ATTEMPT_STATE_CONFLICT",
            Self::JobAlreadyResolved => "JOB_ALREADY_RESOLVED",
            Self::ResultIdentityConflict => "RESULT_IDENTITY_CONFLICT",
            Self::DispatchOutcomeUnknown => "DISPATCH_OUTCOME_UNKNOWN",
            Self::LaunchIdentityMismatch => "LAUNCH_IDENTITY_MISMATCH",
            Self::ReservationStateConflict => "RESERVATION_STATE_CONFLICT",
            Self::ArtifactIdentityConflict => "ARTIFACT_IDENTITY_CONFLICT",
            Self::ArtifactNotFound => "ARTIFACT_NOT_FOUND",
            Self::ArtifactNotUtf8 => "ARTIFACT_NOT_UTF8",
            Self::OutputLimitExceeded => "OUTPUT_LIMIT_EXCEEDED",
            Self::ReconciliationRequired => "RECONCILIATION_REQUIRED",
            Self::OrphanRemediationDenied => "ORPHAN_REMEDIATION_DENIED",
            Self::WorkspaceCapacityExceeded => "WORKSPACE_CAPACITY_EXCEEDED",
            Self::IoError => "IO_ERROR",
            Self::ToolUnavailable => "TOOL_UNAVAILABLE",
            Self::ToolFailed => "TOOL_FAILED",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCapacity {
    pub scope: String,
    pub active: u32,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holder_job_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holder_workspace_ids: Vec<String>,
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
    /// Durable Runtime operation identity when the failing path has already committed one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
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
            operation_id: None,
        }
    }

    pub fn with_operation_id(mut self, operation_id: impl Into<String>) -> Self {
        self.operation_id = Some(operation_id.into());
        self
    }

    pub fn concurrency(message: impl Into<String>, field: &str, capacity: RuntimeCapacity) -> Self {
        Self {
            code: RuntimeErrorCode::ConcurrencyLimit,
            message: message.into(),
            field: Some(field.to_string()),
            retryable: true,
            retry_after_ms: Some(1_000),
            capacity: Some(Box::new(capacity)),
            operation_id: None,
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

    pub fn is_reconciliation_fatal(&self) -> bool {
        matches!(
            self.code,
            RuntimeErrorCode::RegistryUnavailable
                | RuntimeErrorCode::RegistryBusy
                | RuntimeErrorCode::SchemaVersionUnsupported
                | RuntimeErrorCode::MigrationChecksumMismatch
        ) || (self.code == RuntimeErrorCode::RegistryCorrupt && self.field.is_none())
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
