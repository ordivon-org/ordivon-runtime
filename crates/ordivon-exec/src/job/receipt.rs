use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::validation::{
    metadata_corrupt, metadata_error, validate_identifier, validate_sha256_digest,
};
use super::{JobContractError, JobInternalState, JobStateTransition, JOB_CONTRACT_SCHEMA_VERSION};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperationalEventOrigin {
    SystemObserved,
    SystemDerived,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperationalReceiptEventType {
    RequestReceived,
    AuthorizationAllowed,
    AuthorizationDenied,
    JobRecordCreated,
    RunnerUnitStarted,
    RunnerStarted,
    ProcessStarted,
    OutputLimitReached,
    TimeoutTriggered,
    StopRequested,
    ProcessExited,
    JobTerminal,
    RecoveryObserved,
    RecoveryFailed,
}

impl OperationalReceiptEventType {
    fn requires_job_id(self) -> bool {
        !matches!(self, Self::RequestReceived | Self::AuthorizationDenied)
    }

    fn requires_system_observation(self) -> bool {
        matches!(
            self,
            Self::RunnerUnitStarted
                | Self::RunnerStarted
                | Self::ProcessStarted
                | Self::ProcessExited
                | Self::RecoveryObserved
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationalReceiptEvent {
    pub schema_version: u32,
    pub event_id: String,
    pub operation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub client_request_id: String,
    pub timestamp: String,
    pub actor: String,
    pub event_type: OperationalReceiptEventType,
    pub origin: OperationalEventOrigin,
    pub request_digest: String,
    pub policy_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_state: Option<JobInternalState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_state: Option<JobInternalState>,
    pub reason_code: String,
    pub detail_digest: String,
}

impl OperationalReceiptEvent {
    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        if self.schema_version != JOB_CONTRACT_SCHEMA_VERSION {
            return Err(metadata_corrupt(
                "unsupported operational receipt schema version",
                "schemaVersion",
            ));
        }
        for (value, field) in [
            (&self.event_id, "eventId"),
            (&self.operation_id, "operationId"),
            (&self.client_request_id, "clientRequestId"),
            (&self.timestamp, "timestamp"),
            (&self.actor, "actor"),
            (&self.reason_code, "reasonCode"),
        ] {
            validate_identifier(value, field).map_err(|error| metadata_error(error, field))?;
        }
        validate_sha256_digest(&self.request_digest, "requestDigest")
            .map_err(|error| metadata_error(error, "requestDigest"))?;
        validate_sha256_digest(&self.policy_digest, "policyDigest")
            .map_err(|error| metadata_error(error, "policyDigest"))?;
        validate_sha256_digest(&self.detail_digest, "detailDigest")
            .map_err(|error| metadata_error(error, "detailDigest"))?;
        if self.event_type.requires_job_id() && self.job_id.as_deref().is_none_or(str::is_empty) {
            return Err(metadata_corrupt("event type requires a jobId", "jobId"));
        }
        if self.event_type.requires_system_observation()
            && self.origin != OperationalEventOrigin::SystemObserved
        {
            return Err(metadata_corrupt(
                "observed runtime events require SYSTEM_OBSERVED origin",
                "origin",
            ));
        }
        match (self.previous_state, self.new_state) {
            (Some(previous), Some(next)) => JobStateTransition {
                from: previous,
                to: next,
                at: self.timestamp.clone(),
                reason_code: self.reason_code.clone(),
            }
            .validate()?,
            (None, None) => {}
            _ => {
                return Err(metadata_corrupt(
                    "state transition evidence requires both previousState and newState",
                    "previousState",
                ));
            }
        }
        Ok(())
    }
}
