use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::validation::{
    metadata_corrupt, metadata_error, validate_identifier, validate_sha256_digest,
};
use super::{JobContractError, JobContractErrorCode, JOB_CONTRACT_SCHEMA_VERSION};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPublicState {
    Queued,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Lost,
    Orphaned,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobInternalState {
    Accepted,
    Starting,
    Running,
    Stopping,
    Recovering,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Lost,
    Orphaned,
}

impl JobInternalState {
    pub fn public_state(self) -> JobPublicState {
        match self {
            Self::Accepted | Self::Starting => JobPublicState::Queued,
            Self::Running | Self::Stopping | Self::Recovering => JobPublicState::Running,
            Self::Succeeded => JobPublicState::Succeeded,
            Self::Failed => JobPublicState::Failed,
            Self::TimedOut => JobPublicState::TimedOut,
            Self::Cancelled => JobPublicState::Cancelled,
            Self::Lost => JobPublicState::Lost,
            Self::Orphaned => JobPublicState::Orphaned,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::TimedOut
                | Self::Cancelled
                | Self::Lost
                | Self::Orphaned
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Accepted => matches!(
                next,
                Self::Starting | Self::Recovering | Self::Failed | Self::Lost
            ),
            Self::Starting => matches!(
                next,
                Self::Running | Self::Recovering | Self::Failed | Self::Lost | Self::Orphaned
            ),
            Self::Running => matches!(
                next,
                Self::Stopping
                    | Self::Recovering
                    | Self::Succeeded
                    | Self::Failed
                    | Self::TimedOut
                    | Self::Lost
                    | Self::Orphaned
            ),
            Self::Stopping => matches!(
                next,
                Self::Recovering | Self::Cancelled | Self::Failed | Self::Lost | Self::Orphaned
            ),
            Self::Recovering => matches!(
                next,
                Self::Starting
                    | Self::Running
                    | Self::Stopping
                    | Self::Succeeded
                    | Self::Failed
                    | Self::TimedOut
                    | Self::Cancelled
                    | Self::Lost
                    | Self::Orphaned
            ),
            Self::Succeeded
            | Self::Failed
            | Self::TimedOut
            | Self::Cancelled
            | Self::Lost
            | Self::Orphaned => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobStateTransition {
    pub from: JobInternalState,
    pub to: JobInternalState,
    pub at: String,
    pub reason_code: String,
}

impl JobStateTransition {
    pub fn validate(&self) -> Result<(), JobContractError> {
        if !self.from.can_transition_to(self.to) {
            return Err(JobContractError::new(
                JobContractErrorCode::JobStateConflict,
                format!(
                    "invalid job state transition {:?} -> {:?}",
                    self.from, self.to
                ),
                Some("state"),
                false,
            ));
        }
        validate_identifier(&self.at, "at")?;
        validate_identifier(&self.reason_code, "reasonCode")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobOutputMetadata {
    pub generation: u64,
    pub retained_bytes: u64,
    pub dropped_bytes: u64,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

impl JobOutputMetadata {
    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        if let Some(digest) = &self.digest {
            validate_sha256_digest(digest, "digest")
                .map_err(|error| metadata_error(error, "digest"))?;
        }
        if self.dropped_bytes > 0 && !self.truncated {
            return Err(JobContractError::new(
                JobContractErrorCode::JobMetadataCorrupt,
                "dropped output bytes require truncated=true",
                Some("output"),
                false,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobRecord {
    pub schema_version: u32,
    pub job_id: String,
    pub client_request_id: String,
    pub request_digest: String,
    pub policy_id: String,
    pub policy_version: String,
    pub policy_digest: String,
    pub profile_id: String,
    pub principal: String,
    pub authority_ref: String,
    pub internal_state: JobInternalState,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_start_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination_reason: Option<String>,
    pub stdout: JobOutputMetadata,
    pub stderr: JobOutputMetadata,
}

impl JobRecord {
    pub fn public_state(&self) -> JobPublicState {
        self.internal_state.public_state()
    }

    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        if self.schema_version != JOB_CONTRACT_SCHEMA_VERSION {
            return Err(metadata_corrupt(
                "unsupported job record schema version",
                "schemaVersion",
            ));
        }
        for (value, field) in [
            (&self.job_id, "jobId"),
            (&self.client_request_id, "clientRequestId"),
            (&self.policy_id, "policyId"),
            (&self.policy_version, "policyVersion"),
            (&self.profile_id, "profileId"),
            (&self.principal, "principal"),
            (&self.authority_ref, "authorityRef"),
            (&self.created_at, "createdAt"),
        ] {
            validate_identifier(value, field).map_err(|error| metadata_error(error, field))?;
        }
        validate_sha256_digest(&self.request_digest, "requestDigest")
            .map_err(|error| metadata_error(error, "requestDigest"))?;
        validate_sha256_digest(&self.policy_digest, "policyDigest")
            .map_err(|error| metadata_error(error, "policyDigest"))?;
        self.stdout.validate_shape()?;
        self.stderr.validate_shape()?;

        if self.internal_state.is_terminal() {
            if self.finished_at.as_deref().is_none_or(str::is_empty)
                || self.termination_reason.as_deref().is_none_or(str::is_empty)
            {
                return Err(metadata_corrupt(
                    "terminal jobs require finishedAt and terminationReason",
                    "internalState",
                ));
            }
        } else if self.finished_at.is_some() || self.termination_reason.is_some() {
            return Err(metadata_corrupt(
                "non-terminal jobs cannot carry terminal metadata",
                "internalState",
            ));
        }

        if matches!(
            self.internal_state,
            JobInternalState::Running | JobInternalState::Stopping
        ) && (self.started_at.as_deref().is_none_or(str::is_empty)
            || self.unit_name.as_deref().is_none_or(str::is_empty))
        {
            return Err(metadata_corrupt(
                "running or stopping jobs require startedAt and unitName",
                "internalState",
            ));
        }
        if self.runner_pid.is_some()
            != self
                .process_start_identity
                .as_deref()
                .is_some_and(|value| !value.is_empty())
        {
            return Err(metadata_corrupt(
                "runnerPid and processStartIdentity must appear together",
                "runnerPid",
            ));
        }
        if matches!(self.internal_state, JobInternalState::Succeeded) && self.exit_code != Some(0) {
            return Err(metadata_corrupt(
                "succeeded jobs require exitCode=0",
                "exitCode",
            ));
        }
        Ok(())
    }
}
