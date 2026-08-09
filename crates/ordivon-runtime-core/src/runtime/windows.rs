//! Windows-native execution target launched from WSL.
//!
//! Runtime retains Job/Attempt authority.  systemd temporarily supervises the WSL interop
//! launcher lifetime, while the launcher owns the Windows Job Object and emits Windows-native
//! child identity into the Attempt bundle.  The Windows identity is deliberately separate from
//! the outer systemd supervisor identity; later recovery work can promote it without lying about
//! which substrate each observation came from.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde::Deserialize;

use super::{ExecutionBudget, RuntimeError, RuntimeErrorCode, RuntimeResult};

const WINDOWS_BASELINE_ENVIRONMENT_NAMES: &[&str] = &[
    "APPDATA",
    "CommonProgramFiles",
    "CommonProgramW6432",
    "COMPUTERNAME",
    "ComSpec",
    "HOMEDRIVE",
    "HOMEPATH",
    "LOCALAPPDATA",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "Path",
    "PATHEXT",
    "PROCESSOR_ARCHITECTURE",
    "ProgramData",
    "ProgramFiles",
    "ProgramW6432",
    "PUBLIC",
    "SystemDrive",
    "SystemRoot",
    "TEMP",
    "TMP",
    "USERDOMAIN",
    "USERNAME",
    "USERPROFILE",
    "windir",
];

const REQUIRED_WINDOWS_BASELINE_ENVIRONMENT_NAMES: &[&str] = &[
    "ComSpec",
    "Path",
    "PATHEXT",
    "SystemDrive",
    "SystemRoot",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "LOCALAPPDATA",
    "APPDATA",
    "ProgramData",
    "ProgramFiles",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsExecutionConfig {
    /// WSL-visible path to the exact Windows launcher executable, normally under /mnt/<drive>/.
    pub launcher_path: PathBuf,
    /// Explicit WSL distribution authority used to project Linux Workspace paths through
    /// \\wsl.localhost.  This is provider configuration, never inherited process environment.
    pub wsl_distribution: String,
}

impl WindowsExecutionConfig {
    pub(crate) fn validate(&self) -> RuntimeResult<()> {
        if !self.launcher_path.is_absolute() {
            return Err(RuntimeError::invalid(
                "Windows launcher path must be absolute",
                "windows.launcherPath",
            ));
        }
        let metadata = fs::symlink_metadata(&self.launcher_path).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::IoError,
                format!("inspect Windows launcher: {error}"),
                Some("windows.launcherPath"),
                false,
            )
        })?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.permissions().mode() & 0o111 == 0
        {
            return Err(RuntimeError::invalid(
                "Windows launcher must be a non-symlink executable file",
                "windows.launcherPath",
            ));
        }
        mounted_windows_path(&self.launcher_path).ok_or_else(|| {
            RuntimeError::invalid(
                "Windows launcher must reside on a WSL-mounted Windows drive",
                "windows.launcherPath",
            )
        })?;
        if self.wsl_distribution.is_empty()
            || !self
                .wsl_distribution
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(RuntimeError::invalid(
                "WSL distribution name contains unsupported characters",
                "windows.wslDistribution",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WindowsRuntimeContextSnapshot {
    pub schema_version: u32,
    pub token_selection: String,
    pub token_user_sid: String,
    pub token_type: i32,
    pub token_elevation_type: i32,
    pub token_is_elevated: bool,
    pub token_integrity_level_rid: i32,
    pub token_is_restricted: bool,
    pub administrators_group_attributes: u32,
    pub environment: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WindowsStartEvidence {
    pub schema_version: u32,
    pub job_id: String,
    pub attempt_id: String,
    pub launch_token_digest: String,
    pub job_name: String,
    pub launcher_process_id: u32,
    pub process_id: u32,
    pub process_creation_time_file_time: u64,
    pub image_path: String,
    pub image_digest: String,
    pub token_selection: String,
    pub token_user_sid: String,
    pub token_type: i32,
    pub token_elevation_type: i32,
    pub token_is_elevated: bool,
    pub token_integrity_level_rid: i32,
    pub token_is_restricted: bool,
    pub administrators_group_attributes: u32,
    pub observed_unix_ms: u64,
}

pub(crate) struct WindowsSystemdRunSpec<'a> {
    pub config: &'a WindowsExecutionConfig,
    pub unit_name: &'a str,
    pub bundle_path: &'a Path,
    pub job_id: &'a str,
    pub attempt_id: &'a str,
    pub launch_token_digest: &'a str,
    pub executable: &'a Path,
    pub args: &'a [String],
    pub cwd: &'a Path,
    pub environment: &'a BTreeMap<String, String>,
    pub budget: &'a ExecutionBudget,
    pub runtime_ceiling_ms: u64,
    pub timeout_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
}

pub(crate) fn snapshot_windows_runtime_context(
    config: &WindowsExecutionConfig,
) -> RuntimeResult<WindowsRuntimeContextSnapshot> {
    config.validate()?;
    let launcher = fs::canonicalize(&config.launcher_path).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!("canonicalize Windows launcher: {error}"),
            Some("windows.launcherPath"),
            false,
        )
    })?;
    let mut command = Command::new(launcher);
    command.arg("--describe-runtime-context");
    for name in WINDOWS_BASELINE_ENVIRONMENT_NAMES {
        command.arg("--context-env").arg(name);
    }
    let output = command
        .output()
        .map_err(|error| tool_error("describe Windows runtime context", error))?;
    if !output.status.success() {
        return Err(RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!(
                "Windows runtime context probe failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            Some("windows.runtimeContext"),
            true,
        ));
    }
    if output.stdout.len() > 64 * 1024 || output.stderr.len() > 64 * 1024 {
        return Err(RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            "Windows runtime context probe output exceeded bounded size",
            Some("windows.runtimeContext"),
            false,
        ));
    }
    let snapshot: WindowsRuntimeContextSnapshot =
        serde_json::from_slice(&output.stdout).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::InvalidRequest,
                format!("invalid Windows runtime context evidence: {error}"),
                Some("windows.runtimeContext"),
                false,
            )
        })?;
    validate_windows_runtime_context(&snapshot)?;
    Ok(snapshot)
}

fn validate_windows_runtime_context(snapshot: &WindowsRuntimeContextSnapshot) -> RuntimeResult<()> {
    if snapshot.schema_version != 1
        || snapshot.token_user_sid.is_empty()
        || snapshot.token_type != 1
        || snapshot.token_is_elevated
        || snapshot.token_integrity_level_rid > 8192
        || (snapshot.administrators_group_attributes != u32::MAX
            && (snapshot.administrators_group_attributes & 0x4) != 0
            && (snapshot.administrators_group_attributes & 0x10) == 0)
        || !matches!(
            snapshot.token_selection.as_str(),
            "lua_medium_filtered" | "current_limited"
        )
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            "Windows runtime context did not prove limited execution authority",
            Some("windows.runtimeContext"),
            false,
        ));
    }
    for name in REQUIRED_WINDOWS_BASELINE_ENVIRONMENT_NAMES {
        if !snapshot
            .environment
            .iter()
            .any(|(actual, value)| actual.eq_ignore_ascii_case(name) && !value.is_empty())
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::InvalidRequest,
                format!("Windows runtime context omitted required baseline variable {name}"),
                Some("windows.runtimeContext.environment"),
                false,
            ));
        }
    }
    for name in snapshot.environment.keys() {
        if !WINDOWS_BASELINE_ENVIRONMENT_NAMES
            .iter()
            .any(|allowed| name.eq_ignore_ascii_case(allowed))
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::InvalidRequest,
                format!("Windows runtime context returned non-allowlisted variable {name}"),
                Some("windows.runtimeContext.environment"),
                false,
            ));
        }
    }
    Ok(())
}

pub(crate) fn merge_windows_environment(
    baseline: &BTreeMap<String, String>,
    overlay: &BTreeMap<String, String>,
) -> RuntimeResult<BTreeMap<String, String>> {
    let mut seen = Vec::<String>::new();
    for name in overlay.keys() {
        if name.is_empty() || name.contains('=') || name.contains('\0') {
            return Err(RuntimeError::invalid(
                "invalid Windows environment variable name",
                "execution.env",
            ));
        }
        if seen
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(name))
        {
            return Err(RuntimeError::invalid(
                "Windows environment variable names must be unique case-insensitively",
                "execution.env",
            ));
        }
        seen.push(name.clone());
    }
    let mut result = baseline.clone();
    for (name, value) in overlay {
        if let Some(existing) = result
            .keys()
            .find(|existing| existing.eq_ignore_ascii_case(name))
            .cloned()
        {
            result.remove(&existing);
        }
        result.insert(name.clone(), value.clone());
    }
    Ok(result)
}

pub(crate) fn windows_systemd_run(spec: &WindowsSystemdRunSpec<'_>) -> RuntimeResult<Output> {
    build_windows_systemd_run_command(spec)?
        .output()
        .map_err(|error| tool_error("cannot execute Windows systemd-run", error))
}

pub(crate) fn build_windows_systemd_run_command(
    spec: &WindowsSystemdRunSpec<'_>,
) -> RuntimeResult<Command> {
    spec.config.validate()?;
    let launcher = fs::canonicalize(&spec.config.launcher_path).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!("canonicalize Windows launcher: {error}"),
            Some("windows.launcherPath"),
            false,
        )
    })?;
    let executable = mounted_windows_path(spec.executable).ok_or_else(|| {
        RuntimeError::invalid(
            "windows_native executable must reside on a WSL-mounted Windows drive",
            "execution.executable",
        )
    })?;
    let cwd = windows_visible_path(spec.config, spec.cwd, "execution.cwdRelative")?;
    let bundle = windows_visible_path(spec.config, spec.bundle_path, "bundlePath")?;
    let job_name = format!("Ordivon.{}", spec.attempt_id);

    let mut command = Command::new("systemd-run");
    command
        .arg(format!("--unit={}", spec.unit_name))
        .arg("--no-block")
        .args([
            "--property=Type=exec",
            "--property=CollectMode=inactive",
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=2s",
            "--property=SendSIGKILL=yes",
            "--property=StandardOutput=journal",
            "--property=StandardError=journal",
        ])
        .arg(format!(
            "--property=RuntimeMaxSec={}ms",
            spec.runtime_ceiling_ms
        ))
        .arg(launcher)
        .arg("--runtime-bundle")
        .arg(bundle)
        .arg("--runtime-job-id")
        .arg(spec.job_id)
        .arg("--runtime-attempt-id")
        .arg(spec.attempt_id)
        .arg("--runtime-launch-token-digest")
        .arg(spec.launch_token_digest)
        .arg("--job-name")
        .arg(job_name)
        .arg("--timeout-ms")
        .arg(spec.timeout_ms.to_string())
        .arg("--stdout-limit-bytes")
        .arg(spec.stdout_limit_bytes.to_string())
        .arg("--stderr-limit-bytes")
        .arg(spec.stderr_limit_bytes.to_string())
        .arg("--executable")
        .arg(executable)
        .arg("--cwd")
        .arg(cwd)
        .arg("--inherit-environment")
        .arg("false");

    for (name, value) in spec.environment {
        command.arg("--env").arg(format!("{name}={value}"));
    }
    if let Some(value) = spec.budget.memory_max_bytes {
        command.arg("--memory-max-bytes").arg(value.to_string());
    }
    if let Some(value) = spec.budget.tasks_max {
        command.arg("--active-process-limit").arg(value.to_string());
    }
    if let Some(value) = spec.budget.cpu_quota_percent {
        command.arg("--cpu-quota-percent").arg(value.to_string());
    }
    command.arg("--").args(spec.args);
    Ok(command)
}

pub(crate) fn windows_visible_path(
    config: &WindowsExecutionConfig,
    path: &Path,
    field: &str,
) -> RuntimeResult<String> {
    if let Some(path) = mounted_windows_path(path) {
        return Ok(path);
    }
    if !path.is_absolute() {
        return Err(RuntimeError::invalid(
            "Windows-visible path source must be absolute",
            field,
        ));
    }
    let text = path
        .to_str()
        .ok_or_else(|| RuntimeError::invalid("Windows-visible path must be UTF-8", field))?;
    let relative = text.trim_start_matches('/').replace('/', "\\");
    Ok(format!(
        "\\\\wsl.localhost\\{}\\{}",
        config.wsl_distribution, relative
    ))
}

pub(crate) fn mounted_windows_path(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let remainder = text.strip_prefix("/mnt/")?;
    let bytes = remainder.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b'/' {
        return None;
    }
    let drive = (bytes[0] as char).to_ascii_uppercase();
    let tail = remainder[2..].replace('/', "\\");
    if tail.is_empty() {
        Some(format!("{drive}:\\"))
    } else {
        Some(format!("{drive}:\\{tail}"))
    }
}

fn tool_error(operation: &str, error: std::io::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::IoError,
        format!("{operation}: {error}"),
        None,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounted_drive_and_unc_projection_are_explicit() {
        assert_eq!(
            mounted_windows_path(Path::new("/mnt/c/Windows/System32/cmd.exe")).as_deref(),
            Some("C:\\Windows\\System32\\cmd.exe")
        );
        let config = WindowsExecutionConfig {
            launcher_path: PathBuf::from("/mnt/c/launcher.exe"),
            wsl_distribution: "archlinux".to_string(),
        };
        assert_eq!(
            windows_visible_path(
                &config,
                Path::new("/var/lib/ordivon/runtime/workspaces/w"),
                "cwd"
            )
            .unwrap(),
            "\\\\wsl.localhost\\archlinux\\var\\lib\\ordivon\\runtime\\workspaces\\w"
        );
    }

    #[test]
    fn windows_environment_overlay_is_case_insensitive_and_does_not_duplicate_baseline_keys() {
        let baseline = BTreeMap::from([
            ("Path".to_string(), "C:\\Windows\\System32".to_string()),
            ("SystemRoot".to_string(), "C:\\Windows".to_string()),
        ]);
        let overlay = BTreeMap::from([
            ("PATH".to_string(), "C:\\Agent\\Bin".to_string()),
            ("AgentFlag".to_string(), "1".to_string()),
        ]);
        let merged = merge_windows_environment(&baseline, &overlay).unwrap();
        assert_eq!(
            merged.get("PATH").map(String::as_str),
            Some("C:\\Agent\\Bin")
        );
        assert!(!merged.contains_key("Path"));
        assert_eq!(
            merged.get("SystemRoot").map(String::as_str),
            Some("C:\\Windows")
        );
        assert_eq!(merged.get("AgentFlag").map(String::as_str), Some("1"));
    }

    #[test]
    fn windows_runtime_context_rejects_elevated_or_ambient_environment_claims() {
        let mut snapshot = WindowsRuntimeContextSnapshot {
            schema_version: 1,
            token_selection: "lua_medium_filtered".to_string(),
            token_user_sid: "S-1-5-21-test-1001".to_string(),
            token_type: 1,
            token_elevation_type: 2,
            token_is_elevated: false,
            token_integrity_level_rid: 8192,
            token_is_restricted: false,
            administrators_group_attributes: 0x10,
            environment: REQUIRED_WINDOWS_BASELINE_ENVIRONMENT_NAMES
                .iter()
                .map(|name| ((*name).to_string(), format!("value:{name}")))
                .collect(),
        };
        validate_windows_runtime_context(&snapshot).unwrap();
        snapshot.token_is_elevated = true;
        assert!(validate_windows_runtime_context(&snapshot).is_err());
        snapshot.token_is_elevated = false;
        snapshot
            .environment
            .insert("PNPM_HOME".to_string(), "C:\\pnpm".to_string());
        assert!(validate_windows_runtime_context(&snapshot).is_err());
    }
}
