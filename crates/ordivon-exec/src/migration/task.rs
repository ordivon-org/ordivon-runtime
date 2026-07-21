use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    invalid, validate_digest, validate_identifier, validate_message, MigrationBackend,
    MigrationContractError, MigrationContractErrorCode, MAX_INPUT_OPTIONS, MAX_POLL_AFTER_MS,
    MAX_TASK_ARTIFACTS, MIGRATION_CONTRACT_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MigrationTaskStatus {
    Working,
    InputRequired,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskInputRequest {
    pub kind: String,
    pub summary: String,
    #[serde(default)]
    pub options: Vec<String>,
}

impl TaskInputRequest {
    fn validate_shape(&self) -> Result<(), MigrationContractError> {
        validate_identifier(&self.kind, "requiredInput.kind")?;
        validate_message(&self.summary, "requiredInput.summary")?;
        if self.options.len() > MAX_INPUT_OPTIONS {
            return Err(invalid(
                format!("input request supports at most {MAX_INPUT_OPTIONS} options"),
                "requiredInput.options",
            ));
        }
        for option in &self.options {
            validate_message(option, "requiredInput.options")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtifactKind {
    Stdout,
    Stderr,
    Diff,
    TestReport,
    ExecutionResult,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactReference {
    pub artifact_id: String,
    pub task_id: String,
    pub kind: ArtifactKind,
    pub digest: String,
    pub media_type: String,
    pub byte_length: u64,
}

impl ArtifactReference {
    pub fn validate_shape(&self) -> Result<(), MigrationContractError> {
        validate_identifier(&self.artifact_id, "artifactId")?;
        validate_identifier(&self.task_id, "taskId")?;
        validate_digest(&self.digest, "digest")?;
        validate_identifier(&self.media_type, "mediaType")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationTaskHandle {
    pub schema_version: u32,
    pub task_id: String,
    pub backend: MigrationBackend,
    pub status: MigrationTaskStatus,
    pub status_message: String,
    pub result_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_input: Option<TaskInputRequest>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactReference>,
}

impl MigrationTaskHandle {
    pub fn validate_shape(&self) -> Result<(), MigrationContractError> {
        if self.schema_version != MIGRATION_CONTRACT_SCHEMA_VERSION {
            return Err(invalid(
                "unsupported migration schema version",
                "schemaVersion",
            ));
        }
        validate_identifier(&self.task_id, "taskId")?;
        validate_message(&self.status_message, "statusMessage")?;
        if let Some(cursor) = &self.event_cursor {
            validate_identifier(cursor, "eventCursor")?;
        }
        if self.artifacts.len() > MAX_TASK_ARTIFACTS {
            return Err(invalid(
                format!("task supports at most {MAX_TASK_ARTIFACTS} artifact references"),
                "artifacts",
            ));
        }
        for artifact in &self.artifacts {
            artifact.validate_shape()?;
            if artifact.task_id != self.task_id {
                return Err(MigrationContractError::new(
                    MigrationContractErrorCode::InvalidArtifact,
                    "artifact reference belongs to a different task",
                    "artifacts",
                ));
            }
        }

        let terminal = matches!(
            self.status,
            MigrationTaskStatus::Completed
                | MigrationTaskStatus::Failed
                | MigrationTaskStatus::Cancelled
        );
        if terminal != self.result_available {
            return Err(MigrationContractError::new(
                MigrationContractErrorCode::InvalidTaskState,
                "terminal tasks must expose a result and non-terminal tasks must not",
                "resultAvailable",
            ));
        }
        match self.status {
            MigrationTaskStatus::Working => {
                let poll = self.poll_after_ms.ok_or_else(|| {
                    MigrationContractError::new(
                        MigrationContractErrorCode::InvalidTaskState,
                        "working tasks require pollAfterMs",
                        "pollAfterMs",
                    )
                })?;
                if poll == 0 || poll > MAX_POLL_AFTER_MS {
                    return Err(invalid(
                        format!("pollAfterMs must be in 1..={MAX_POLL_AFTER_MS}"),
                        "pollAfterMs",
                    ));
                }
                if self.required_input.is_some() {
                    return Err(invalid(
                        "working tasks cannot request input",
                        "requiredInput",
                    ));
                }
            }
            MigrationTaskStatus::InputRequired => {
                let input = self.required_input.as_ref().ok_or_else(|| {
                    MigrationContractError::new(
                        MigrationContractErrorCode::InvalidTaskState,
                        "input-required tasks must explain the required input",
                        "requiredInput",
                    )
                })?;
                input.validate_shape()?;
                if self.poll_after_ms.is_some() {
                    return Err(invalid(
                        "input-required tasks must not ask the model to poll",
                        "pollAfterMs",
                    ));
                }
            }
            MigrationTaskStatus::Completed
            | MigrationTaskStatus::Failed
            | MigrationTaskStatus::Cancelled => {
                if self.poll_after_ms.is_some() || self.required_input.is_some() {
                    return Err(invalid(
                        "terminal tasks cannot poll or request input",
                        "status",
                    ));
                }
            }
        }
        Ok(())
    }
}
