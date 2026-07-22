use std::fs;
use std::path::{Path, PathBuf};

use super::{invalid, UniversalExecError, UniversalExecErrorCode};

pub const UNIVERSAL_EXEC_SCHEMA_VERSION: u32 = 1;
pub const MAX_UNIVERSAL_ARGS: usize = 128;
pub const MAX_UNIVERSAL_ARG_BYTES: usize = 16 * 1024;
pub const MAX_UNIVERSAL_ENV_VARS: usize = 64;
pub const MAX_UNIVERSAL_ENV_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_UNIVERSAL_RUNTIME_MS: u64 = 24 * 60 * 60 * 1000;
pub const MAX_UNIVERSAL_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_WORKSPACE_MUTATIONS: usize = 32;
pub const MAX_WORKSPACE_IO_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct UniversalExecutorConfig {
    pub store_root: PathBuf,
    pub workspace_root: Option<PathBuf>,
    pub workspace_uid: Option<u32>,
    pub workspace_gid: Option<u32>,
    pub runner_path: PathBuf,
    pub allowed_executable_roots: Vec<PathBuf>,
    pub max_runtime_ms: u64,
    pub max_output_bytes: u64,
}

impl UniversalExecutorConfig {
    pub fn validate(&self) -> Result<(), UniversalExecError> {
        if !self.store_root.is_absolute() {
            return Err(invalid("store root must be absolute", "storeRoot"));
        }
        if self
            .workspace_root
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err(invalid("workspace root must be absolute", "workspaceRoot"));
        }
        if self.workspace_uid.is_some() != self.workspace_gid.is_some() {
            return Err(invalid(
                "workspaceUid and workspaceGid must appear together",
                "workspaceUid",
            ));
        }
        if self.workspace_uid == Some(0) || self.workspace_gid == Some(0) {
            return Err(invalid("workspace owner must be non-root", "workspaceUid"));
        }
        if !self.runner_path.is_absolute() {
            return Err(invalid("runner path must be absolute", "runnerPath"));
        }
        if self.allowed_executable_roots.is_empty() {
            return Err(invalid(
                "at least one executable root is required",
                "allowedExecutableRoots",
            ));
        }
        if self
            .allowed_executable_roots
            .iter()
            .any(|root| !root.is_absolute())
        {
            return Err(invalid(
                "all executable roots must be absolute",
                "allowedExecutableRoots",
            ));
        }
        if self.max_runtime_ms == 0 || self.max_runtime_ms > MAX_UNIVERSAL_RUNTIME_MS {
            return Err(invalid(
                format!("max runtime must be in 1..={MAX_UNIVERSAL_RUNTIME_MS}"),
                "maxRuntimeMs",
            ));
        }
        if self.max_output_bytes == 0 || self.max_output_bytes > MAX_UNIVERSAL_OUTPUT_BYTES {
            return Err(invalid(
                format!("max output must be in 1..={MAX_UNIVERSAL_OUTPUT_BYTES}"),
                "maxOutputBytes",
            ));
        }
        Ok(())
    }

    pub fn ensure_store(&self) -> Result<(), UniversalExecError> {
        self.validate()?;
        for path in [
            self.store_root.clone(),
            self.workspaces_root(),
            self.workspace_records_root(),
            self.tasks_root(),
        ] {
            fs::create_dir_all(&path).map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::IoError,
                    format!("cannot create {}: {error}", path.display()),
                    Some("storeRoot"),
                    false,
                )
            })?;
        }
        Ok(())
    }

    pub fn workspaces_root(&self) -> PathBuf {
        self.workspace_root
            .clone()
            .unwrap_or_else(|| self.store_root.join("workspaces"))
    }

    pub fn workspace_records_root(&self) -> PathBuf {
        self.store_root.join("workspace-records")
    }

    pub fn tasks_root(&self) -> PathBuf {
        self.store_root.join("tasks")
    }

    pub(crate) fn workspace_path(&self, workspace_id: &str) -> PathBuf {
        self.workspaces_root().join(workspace_id)
    }

    pub(crate) fn workspace_record_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_records_root()
            .join(format!("{workspace_id}.json"))
    }
}

pub(crate) fn canonical_directory(path: &Path, field: &str) -> Result<PathBuf, UniversalExecError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("cannot canonicalize {}: {error}", path.display()),
            Some(field),
            false,
        )
    })?;
    if !canonical.is_dir() {
        return Err(invalid(format!("{field} must be a directory"), field));
    }
    Ok(canonical)
}
