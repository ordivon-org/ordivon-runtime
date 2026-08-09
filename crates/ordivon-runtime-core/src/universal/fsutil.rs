use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{UniversalExecError, UniversalExecErrorCode};

#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RESOLVE_BENEATH: u64 = 0x08;

pub(crate) fn open_directory_nofollow(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
}

pub(crate) fn open_regular_file_beneath(
    root_file: &File,
    relative: &Path,
    deny_parent_symlinks: bool,
) -> std::io::Result<File> {
    let relative = CString::new(relative.as_os_str().as_encoded_bytes()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "relative path contains NUL",
        )
    })?;
    let mut resolve = RESOLVE_NO_MAGICLINKS | RESOLVE_BENEATH;
    if deny_parent_symlinks {
        resolve |= RESOLVE_NO_SYMLINKS;
    }
    let how = OpenHow {
        flags: (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK) as u64,
        mode: 0,
        resolve,
    };
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            root_file.as_raw_fd(),
            relative.as_ptr(),
            &how,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let file = unsafe { File::from_raw_fd(fd as i32) };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "resolved object is not a regular file",
        ));
    }
    Ok(file)
}

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

pub(crate) fn linux_exec_string_limit_bytes() -> Result<usize, UniversalExecError> {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return Err(invalid(
            "cannot determine Linux execve per-string limit",
            "execution",
        ));
    }
    usize::try_from(page_size)
        .ok()
        .and_then(|page_size| page_size.checked_mul(32))
        .and_then(|bytes| bytes.checked_sub(1))
        .ok_or_else(|| {
            invalid(
                "Linux execve per-string limit is not representable",
                "execution",
            )
        })
}

pub(crate) fn linux_exec_payload_limit_bytes() -> Result<usize, UniversalExecError> {
    let arg_max = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
    if arg_max <= 0 {
        return Err(invalid(
            "cannot determine Linux execve argv/environment limit",
            "execution",
        ));
    }
    usize::try_from(arg_max).map_err(|_| {
        invalid(
            "Linux execve argv/environment limit is not representable",
            "execution",
        )
    })
}

pub(crate) fn validate_args(args: &[String]) -> Result<(), UniversalExecError> {
    let max_string_bytes = linux_exec_string_limit_bytes()?;
    if args
        .iter()
        .any(|arg| arg.len() > max_string_bytes || arg.as_bytes().contains(&0))
    {
        return Err(invalid(
            format!(
                "args contains a value that exceeds the Linux execve per-string limit of {max_string_bytes} bytes or contains NUL"
            ),
            "args",
        ));
    }
    Ok(())
}

pub(crate) fn validate_env(
    env: &std::collections::BTreeMap<String, String>,
) -> Result<(), UniversalExecError> {
    let max_string_bytes = linux_exec_string_limit_bytes()?;
    for (name, value) in env {
        let mut chars = name.chars();
        let valid_name = chars
            .next()
            .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
            && chars.all(|character| character == '_' || character.is_ascii_alphanumeric());
        let encoded_len = name.len().saturating_add(1).saturating_add(value.len());
        if !valid_name || encoded_len > max_string_bytes || value.as_bytes().contains(&0) {
            return Err(invalid(format!("invalid environment entry {name}"), "env"));
        }
    }
    Ok(())
}

pub(crate) fn validate_exec_payload(
    args: &[String],
    env: &std::collections::BTreeMap<String, String>,
    field: &str,
) -> Result<(), UniversalExecError> {
    validate_args(args)?;
    validate_env(env)?;

    let mut string_bytes = 0usize;
    for arg in args {
        string_bytes = string_bytes
            .checked_add(arg.len())
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| invalid("execve payload size overflow", field))?;
    }
    for (name, value) in env {
        string_bytes = string_bytes
            .checked_add(name.len())
            .and_then(|total| total.checked_add(1))
            .and_then(|total| total.checked_add(value.len()))
            .and_then(|total| total.checked_add(1))
            .ok_or_else(|| invalid("execve payload size overflow", field))?;
    }

    // Linux accounts argv/env pointer tables against the same stack-backed exec budget.
    // Include both terminating null pointers so admission stays on the safe side of E2BIG.
    let pointer_count = args
        .len()
        .checked_add(env.len())
        .and_then(|count| count.checked_add(2))
        .ok_or_else(|| invalid("execve pointer count overflow", field))?;
    let pointer_bytes = pointer_count
        .checked_mul(std::mem::size_of::<usize>())
        .ok_or_else(|| invalid("execve pointer size overflow", field))?;
    let required_bytes = string_bytes
        .checked_add(pointer_bytes)
        .ok_or_else(|| invalid("execve payload size overflow", field))?;
    let max_bytes = linux_exec_payload_limit_bytes()?;
    if required_bytes > max_bytes {
        return Err(invalid(
            format!(
                "argv and environment require {required_bytes} bytes, exceeding the host Linux execve limit of {max_bytes} bytes"
            ),
            field,
        ));
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

pub(crate) fn io_error(path: &Path, operation: &str, error: std::io::Error) -> UniversalExecError {
    let code = match error.raw_os_error() {
        Some(libc::ENOSPC) | Some(libc::EDQUOT) => {
            UniversalExecErrorCode::WorkspaceCapacityExceeded
        }
        _ => UniversalExecErrorCode::IoError,
    };
    UniversalExecError::new(
        code,
        format!("cannot {operation} {}: {error}", path.display()),
        None,
        false,
    )
}
