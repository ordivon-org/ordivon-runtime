use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::{M6Error, M6Result};

pub const M7_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M7WorkerIdentity {
    pub user: String,
    pub group: String,
    pub uid: u32,
    pub gid: u32,
}

#[derive(Clone, Debug)]
pub struct M7RuntimeHardeningConfig {
    pub worker: M7WorkerIdentity,
    pub control_root: PathBuf,
    pub worker_root: PathBuf,
    pub cache_root: PathBuf,
    pub runtime_view_root: PathBuf,
}

impl M7RuntimeHardeningConfig {
    pub fn validate(&self) -> M6Result<()> {
        if self.worker.user != "ordivon-worker" || self.worker.group != "ordivon-worker" {
            return Err(M6Error::invalid(
                "M7 worker identity must be ordivon-worker",
                "worker",
            ));
        }
        if self.worker.uid == 0 || self.worker.gid == 0 {
            return Err(M6Error::invalid("M7 worker must be non-root", "worker"));
        }
        for (field, path) in [
            ("controlRoot", &self.control_root),
            ("workerRoot", &self.worker_root),
            ("cacheRoot", &self.cache_root),
            ("runtimeViewRoot", &self.runtime_view_root),
        ] {
            if !path.is_absolute() {
                return Err(M6Error::invalid(format!("{field} must be absolute"), field));
            }
        }
        if self.worker_root.starts_with(&self.control_root)
            || self.cache_root.starts_with(&self.control_root)
            || self.runtime_view_root.starts_with(&self.control_root)
            || self.control_root.starts_with(&self.worker_root)
        {
            return Err(M6Error::invalid(
                "M7 control, worker, and cache roots must not overlap",
                "controlRoot",
            ));
        }
        self.verify_host_identity()?;
        Ok(())
    }

    pub fn workspaces_root(&self) -> PathBuf {
        self.worker_root.join("workspaces")
    }

    pub fn attempts_root(&self) -> PathBuf {
        self.worker_root.join("attempts")
    }

    pub fn payload_runtime_dir(&self, attempt_id: &str) -> PathBuf {
        self.attempts_root().join(attempt_id)
    }

    pub fn payload_view_root(&self, attempt_id: &str) -> PathBuf {
        self.runtime_view_root.join(attempt_id)
    }

    pub fn verify_host_identity(&self) -> M6Result<()> {
        let passwd = fs::read_to_string("/etc/passwd").map_err(|error| {
            M6Error::invalid(format!("cannot read /etc/passwd: {error}"), "worker")
        })?;
        let expected = format!(
            "{}:x:{}:{}:",
            self.worker.user, self.worker.uid, self.worker.gid
        );
        if !passwd.lines().any(|line| line.starts_with(&expected)) {
            return Err(M6Error::invalid(
                "configured M7 worker does not match /etc/passwd",
                "worker",
            ));
        }
        Ok(())
    }
}

pub(crate) fn ensure_owned_directory(path: &Path, uid: u32, gid: u32, mode: u32) -> M6Result<()> {
    fs::create_dir_all(path).map_err(|error| {
        M6Error::invalid(format!("cannot create {}: {error}", path.display()), "path")
    })?;
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| M6Error::invalid("path contains NUL", "path"))?;
    let result = unsafe { libc::chown(c.as_ptr(), uid, gid) };
    if result != 0 {
        return Err(M6Error::invalid(
            format!(
                "cannot chown {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ),
            "path",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        M6Error::invalid(format!("cannot chmod {}: {error}", path.display()), "path")
    })?;
    let metadata = fs::metadata(path).map_err(|error| {
        M6Error::invalid(
            format!("cannot inspect {}: {error}", path.display()),
            "path",
        )
    })?;
    if metadata.uid() != uid
        || metadata.gid() != gid
        || metadata.permissions().mode() & 0o7777 != mode
    {
        return Err(M6Error::invalid(
            "M7 directory ownership verification failed",
            "path",
        ));
    }
    Ok(())
}

pub(crate) fn ensure_traversal_directory(path: &Path, gid: u32, mode: u32) -> M6Result<()> {
    fs::create_dir_all(path).map_err(|error| {
        M6Error::invalid(format!("cannot create {}: {error}", path.display()), "path")
    })?;
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| M6Error::invalid("path contains NUL", "path"))?;
    let result = unsafe { libc::chown(c.as_ptr(), 0, gid) };
    if result != 0 {
        return Err(M6Error::invalid(
            format!(
                "cannot chown {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ),
            "path",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        M6Error::invalid(format!("cannot chmod {}: {error}", path.display()), "path")
    })?;
    Ok(())
}
