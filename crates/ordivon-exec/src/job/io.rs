use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::validation::{invalid, validate_identifier};
use super::{
    JobContractError, JobContractErrorCode, JobPublicState, JobRecord, JOB_CONTRACT_SCHEMA_VERSION,
    MAX_JOB_LIST_LIMIT, MAX_JOB_READ_BYTES,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobOutputStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobOutputEncoding {
    Utf8Lossy,
    Base64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobOutputCursor {
    pub schema_version: u32,
    pub job_id: String,
    pub stream: JobOutputStream,
    pub generation: u64,
    pub byte_offset: u64,
}

impl JobOutputCursor {
    pub fn validate_for(
        &self,
        job_id: &str,
        stream: JobOutputStream,
    ) -> Result<(), JobContractError> {
        if self.schema_version != JOB_CONTRACT_SCHEMA_VERSION
            || self.job_id != job_id
            || self.stream != stream
        {
            return Err(JobContractError::new(
                JobContractErrorCode::CursorInvalid,
                "cursor does not match the requested job stream",
                Some("cursor"),
                false,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobReadRequest {
    pub job_id: String,
    pub stream: JobOutputStream,
    #[serde(default)]
    pub cursor: Option<JobOutputCursor>,
    pub max_bytes: u64,
    pub encoding: JobOutputEncoding,
}

impl JobReadRequest {
    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        validate_identifier(&self.job_id, "jobId")?;
        if self.max_bytes == 0 || self.max_bytes > MAX_JOB_READ_BYTES {
            return Err(invalid(
                format!("maxBytes must be in 1..={MAX_JOB_READ_BYTES}"),
                "maxBytes",
            ));
        }
        if let Some(cursor) = &self.cursor {
            cursor.validate_for(&self.job_id, self.stream)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobReadResult {
    pub data: String,
    pub next_cursor: JobOutputCursor,
    pub retained_end: u64,
    pub job_terminal: bool,
    pub output_truncated: bool,
    pub dropped_bytes: u64,
    pub encoding: JobOutputEncoding,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobListCursor {
    pub schema_version: u32,
    pub created_at: String,
    pub job_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobListRequest {
    pub limit: u32,
    #[serde(default)]
    pub cursor: Option<JobListCursor>,
    #[serde(default)]
    pub states: Vec<JobPublicState>,
    #[serde(default)]
    pub created_after: Option<String>,
    #[serde(default)]
    pub created_before: Option<String>,
}

impl JobListRequest {
    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        if self.limit == 0 || self.limit > MAX_JOB_LIST_LIMIT {
            return Err(invalid(
                format!("limit must be in 1..={MAX_JOB_LIST_LIMIT}"),
                "limit",
            ));
        }
        if let Some(cursor) = &self.cursor {
            if cursor.schema_version != JOB_CONTRACT_SCHEMA_VERSION {
                return Err(JobContractError::new(
                    JobContractErrorCode::CursorInvalid,
                    "unsupported list cursor schema version",
                    Some("cursor"),
                    false,
                ));
            }
            validate_identifier(&cursor.created_at, "cursor.createdAt")?;
            validate_identifier(&cursor.job_id, "cursor.jobId")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobListResult {
    pub jobs: Vec<JobRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<JobListCursor>,
}
