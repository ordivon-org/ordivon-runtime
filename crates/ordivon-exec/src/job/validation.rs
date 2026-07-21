use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{
    JobContractError, JobContractErrorCode, MAX_IDENTIFIER_BYTES, MAX_JOB_ARGS, MAX_JOB_ARG_BYTES,
    MAX_JOB_ENV_VALUE_BYTES, MAX_JOB_ENV_VARS, MAX_JOB_OUTPUT_RETENTION_BYTES,
};

pub(super) fn validate_identifier(value: &str, field: &str) -> Result<(), JobContractError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || value.as_bytes().contains(&0)
    {
        return Err(invalid(
            format!("{field} must be non-empty, bounded, and NUL-free"),
            field,
        ));
    }
    Ok(())
}

pub(super) fn validate_absolute_path(
    value: &str,
    field: &str,
    code: JobContractErrorCode,
) -> Result<(), JobContractError> {
    if value.as_bytes().contains(&0) || !Path::new(value).is_absolute() {
        return Err(JobContractError::new(
            code,
            format!("{field} must be an absolute NUL-free path"),
            Some(field),
            false,
        ));
    }
    Ok(())
}

pub(super) fn validate_argument_vector(
    args: &[String],
    field: &str,
) -> Result<(), JobContractError> {
    if args.len() > MAX_JOB_ARGS {
        return Err(invalid(
            format!("{field} supports at most {MAX_JOB_ARGS} arguments"),
            field,
        ));
    }
    if args
        .iter()
        .any(|argument| argument.len() > MAX_JOB_ARG_BYTES || argument.as_bytes().contains(&0))
    {
        return Err(invalid(
            format!("{field} contains an oversized or NUL-bearing argument"),
            field,
        ));
    }
    Ok(())
}

pub(super) fn validate_environment(
    environment: &BTreeMap<String, String>,
) -> Result<(), JobContractError> {
    if environment.len() > MAX_JOB_ENV_VARS {
        return Err(JobContractError::new(
            JobContractErrorCode::EnvironmentDenied,
            format!("envOverrides supports at most {MAX_JOB_ENV_VARS} entries"),
            Some("envOverrides"),
            false,
        ));
    }
    for (name, value) in environment {
        if !is_valid_environment_name(name)
            || value.len() > MAX_JOB_ENV_VALUE_BYTES
            || value.as_bytes().contains(&0)
        {
            return Err(JobContractError::new(
                JobContractErrorCode::EnvironmentDenied,
                format!("invalid environment override {name}"),
                Some("envOverrides"),
                false,
            ));
        }
    }
    Ok(())
}

pub(super) fn is_valid_environment_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

pub(super) fn validate_retention(value: u64, field: &str) -> Result<(), JobContractError> {
    if value == 0 || value > MAX_JOB_OUTPUT_RETENTION_BYTES {
        return Err(invalid(
            format!("{field} must be in 1..={MAX_JOB_OUTPUT_RETENTION_BYTES}"),
            field,
        ));
    }
    Ok(())
}

pub(super) fn validate_sha256_digest(value: &str, field: &str) -> Result<(), JobContractError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(invalid(format!("{field} must be a sha256 digest"), field));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{field} must be a sha256 digest"), field));
    }
    Ok(())
}

pub(super) fn ensure_unique(values: &[String], field: &str) -> Result<(), JobContractError> {
    let unique: BTreeSet<_> = values.iter().collect();
    if unique.len() != values.len() {
        return Err(policy_invalid(
            format!("{field} entries must be unique"),
            field,
        ));
    }
    Ok(())
}

pub(super) fn invalid(message: impl Into<String>, field: &str) -> JobContractError {
    JobContractError::new(
        JobContractErrorCode::InvalidRequest,
        message,
        Some(field),
        false,
    )
}

pub(super) fn policy_invalid(message: impl Into<String>, field: &str) -> JobContractError {
    JobContractError::new(
        JobContractErrorCode::PolicyInvalid,
        message,
        Some(field),
        false,
    )
}

pub(super) fn policy_error(mut error: JobContractError, field: &str) -> JobContractError {
    error.code = JobContractErrorCode::PolicyInvalid;
    error.field = Some(field.to_string());
    error
}

pub(super) fn metadata_corrupt(message: impl Into<String>, field: &str) -> JobContractError {
    JobContractError::new(
        JobContractErrorCode::JobMetadataCorrupt,
        message,
        Some(field),
        false,
    )
}

pub(super) fn metadata_error(mut error: JobContractError, field: &str) -> JobContractError {
    error.code = JobContractErrorCode::JobMetadataCorrupt;
    error.field = Some(field.to_string());
    error
}
