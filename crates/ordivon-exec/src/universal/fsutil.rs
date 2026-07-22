use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    UniversalExecError, UniversalExecErrorCode, MAX_UNIVERSAL_ARGS, MAX_UNIVERSAL_ARG_BYTES,
    MAX_UNIVERSAL_ENV_VALUE_BYTES, MAX_UNIVERSAL_ENV_VARS,
};

pub(crate) fn validate_id(value: &str, field: &str) -> Result<(), UniversalExecError> {
    let mut chars = value.chars();
    let valid_first = chars
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    if !valid_first
        || value.len() > 96
        || !chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return Err(invalid(
            format!("{field} must match [A-Za-z0-9][A-Za-z0-9._-]{{0,95}}"),
            field,
        ));
    }
    Ok(())
}

pub(crate) fn validate_relative_path(
    value: &str,
    field: &str,
) -> Result<PathBuf, UniversalExecError> {
    let path = Path::new(value);
    if value.is_empty() || path.is_absolute() || value.as_bytes().contains(&0) {
        return Err(invalid(
            format!("{field} must be a non-empty relative path"),
            field,
        ));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspacePathDenied,
            format!("{field} cannot escape the workspace"),
            Some(field),
            false,
        ));
    }
    Ok(path.to_path_buf())
}

pub(crate) fn validate_args(args: &[String]) -> Result<(), UniversalExecError> {
    if args.len() > MAX_UNIVERSAL_ARGS {
        return Err(invalid(
            format!("args supports at most {MAX_UNIVERSAL_ARGS} entries"),
            "args",
        ));
    }
    if args
        .iter()
        .any(|arg| arg.len() > MAX_UNIVERSAL_ARG_BYTES || arg.as_bytes().contains(&0))
    {
        return Err(invalid("args contains an invalid value", "args"));
    }
    Ok(())
}

pub(crate) fn validate_env(
    env: &std::collections::BTreeMap<String, String>,
) -> Result<(), UniversalExecError> {
    if env.len() > MAX_UNIVERSAL_ENV_VARS {
        return Err(invalid(
            format!("env supports at most {MAX_UNIVERSAL_ENV_VARS} entries"),
            "env",
        ));
    }
    for (name, value) in env {
        let mut chars = name.chars();
        let valid_name = chars
            .next()
            .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
            && chars.all(|character| character == '_' || character.is_ascii_alphanumeric());
        if !valid_name
            || value.len() > MAX_UNIVERSAL_ENV_VALUE_BYTES
            || value.as_bytes().contains(&0)
        {
            return Err(invalid(format!("invalid environment entry {name}"), "env"));
        }
    }
    Ok(())
}

pub(crate) fn invalid(message: impl Into<String>, field: impl Into<String>) -> UniversalExecError {
    let field = field.into();
    UniversalExecError::new(
        UniversalExecErrorCode::InvalidRequest,
        message,
        Some(&field),
        false,
    )
}

pub(crate) fn now_unix_ms() -> Result<u128, UniversalExecError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("system clock is before unix epoch: {error}"),
                None,
                false,
            )
        })
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, UniversalExecError> {
    let mut file = File::open(path).map_err(|error| io_error(path, "open", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| io_error(path, "read", error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

pub(crate) fn write_json_atomic(
    path: &Path,
    value: &impl Serialize,
) -> Result<(), UniversalExecError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("cannot serialize {}: {error}", path.display()),
            None,
            false,
        )
    })?;
    write_bytes_atomic(path, &bytes)
}

pub(crate) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), UniversalExecError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("path has no parent", "path"))?;
    fs::create_dir_all(parent).map_err(|error| io_error(parent, "create", error))?;
    let temp = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        std::process::id(),
        now_unix_ms()?
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|error| io_error(&temp, "create", error))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| io_error(&temp, "write", error))?;
    fs::rename(&temp, path).map_err(|error| io_error(path, "rename", error))?;
    sync_directory(parent)
}

pub(crate) fn sync_directory(path: &Path) -> Result<(), UniversalExecError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error(path, "sync directory", error))
}

pub(crate) fn io_error(
    path: &Path,
    operation: &str,
    error: impl std::fmt::Display,
) -> UniversalExecError {
    UniversalExecError::new(
        UniversalExecErrorCode::IoError,
        format!("cannot {operation} {}: {error}", path.display()),
        None,
        false,
    )
}
