use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::{policy_error, policy_io_error};
use crate::job::{JobContractError, JobContractErrorCode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FileIdentity {
    pub(super) device: u64,
    pub(super) inode: u64,
    pub(super) size: u64,
    pub(super) modified_ns: i128,
    pub(super) digest: String,
}

pub(super) fn canonical_roots(
    roots: &[String],
    field: &str,
) -> Result<Vec<PathBuf>, JobContractError> {
    let mut canonical = Vec::with_capacity(roots.len());
    let mut unique = BTreeSet::new();
    for root in roots {
        let path = canonical_directory(root, field, JobContractErrorCode::PolicyInvalid)?;
        if !unique.insert(path.clone()) {
            return Err(policy_error(
                "roots must remain unique after canonicalization",
                field,
            ));
        }
        canonical.push(path);
    }
    canonical.sort();
    Ok(canonical)
}

pub(super) fn canonical_directory(
    path: impl AsRef<Path>,
    field: &str,
    code: JobContractErrorCode,
) -> Result<PathBuf, JobContractError> {
    let canonical = fs::canonicalize(path.as_ref()).map_err(|error| {
        JobContractError::new(
            code.clone(),
            format!("cannot canonicalize {field}: {error}"),
            Some(field),
            false,
        )
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        JobContractError::new(
            code.clone(),
            format!("cannot inspect canonical {field}: {error}"),
            Some(field),
            false,
        )
    })?;
    if !metadata.is_dir() {
        return Err(JobContractError::new(
            code,
            format!("{field} must resolve to a directory"),
            Some(field),
            false,
        ));
    }
    Ok(canonical)
}

pub(super) fn inspect_executable(path: &Path) -> Result<FileIdentity, JobContractError> {
    let before = fs::symlink_metadata(path).map_err(|error| {
        policy_io_error("executable", "cannot inspect profile executable", error)
    })?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(policy_error(
            "profile executable must remain a non-symlink regular file",
            "executable",
        ));
    }
    if before.permissions().mode() & 0o111 == 0 {
        return Err(policy_error(
            "profile executable has no execute permission bits",
            "executable",
        ));
    }
    if before.permissions().mode() & 0o022 != 0 {
        return Err(policy_error(
            "profile executable must not be group- or world-writable",
            "executable",
        ));
    }
    let mut file = File::open(path)
        .map_err(|error| policy_io_error("executable", "cannot open executable", error))?;
    let opened = file.metadata().map_err(|error| {
        policy_io_error("executable", "cannot inspect opened executable", error)
    })?;
    if !same_file(&before, &opened) {
        return Err(policy_error(
            "executable identity changed while it was opened",
            "executable",
        ));
    }
    let digest = digest_reader(&mut file)
        .map_err(|error| policy_io_error("executable", "cannot hash profile executable", error))?;
    Ok(FileIdentity {
        device: opened.dev(),
        inode: opened.ino(),
        size: opened.len(),
        modified_ns: opened.mtime() as i128 * 1_000_000_000 + opened.mtime_nsec() as i128,
        digest,
    })
}

pub(super) fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
}

pub(super) fn digest_reader(reader: &mut impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

pub(super) fn path_to_string(path: &Path, field: &str) -> Result<String, JobContractError> {
    path.to_str().map(ToString::to_string).ok_or_else(|| {
        JobContractError::new(
            JobContractErrorCode::PolicyInvalid,
            format!("canonical {field} is not valid UTF-8"),
            Some(field),
            false,
        )
    })
}
