use super::{MigrationContractError, MigrationContractErrorCode, MAX_STATUS_MESSAGE_BYTES};

pub(crate) fn validate_identifier(value: &str, field: &str) -> Result<(), MigrationContractError> {
    if value.trim().is_empty() || value.len() > 256 || value.as_bytes().contains(&0) {
        return Err(invalid(
            format!("{field} must be non-empty, bounded, and NUL-free"),
            field,
        ));
    }
    Ok(())
}

pub(crate) fn validate_message(value: &str, field: &str) -> Result<(), MigrationContractError> {
    if value.trim().is_empty()
        || value.len() > MAX_STATUS_MESSAGE_BYTES
        || value.as_bytes().contains(&0)
    {
        return Err(invalid(
            format!("{field} must be non-empty, bounded, and NUL-free"),
            field,
        ));
    }
    Ok(())
}

pub(crate) fn validate_digest(value: &str, field: &str) -> Result<(), MigrationContractError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(MigrationContractError::new(
            MigrationContractErrorCode::InvalidArtifact,
            format!("{field} must be a SHA-256 digest"),
            field,
        ));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MigrationContractError::new(
            MigrationContractErrorCode::InvalidArtifact,
            format!("{field} must be a SHA-256 digest"),
            field,
        ));
    }
    Ok(())
}

pub(crate) fn invalid(
    message: impl Into<String>,
    field: impl Into<String>,
) -> MigrationContractError {
    MigrationContractError::new(MigrationContractErrorCode::InvalidContract, message, field)
}
