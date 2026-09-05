//! systemd/cgroup launch, identity, and process-tree helpers for Runtime.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::supervisor::SupervisorIdentity;
use super::{AttemptRecord, RuntimeError, RuntimeErrorCode, RuntimeResult};
use crate::universal::UniversalExecutorConfig;

const SYSTEMCTL_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(1);
const OBSERVATION_COMMAND_POLL: Duration = Duration::from_millis(10);

pub(super) fn validate_executable(
    config: &UniversalExecutorConfig,
    value: &str,
    field: &str,
) -> RuntimeResult<PathBuf> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(RuntimeError::invalid(
            "executable must be an absolute path",
            field,
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!("canonicalize executable: {error}"),
            Some(field),
            false,
        )
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!("inspect executable: {error}"),
            Some(field),
            false,
        )
    })?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(RuntimeError::invalid(
            "executable must resolve to an executable file",
            field,
        ));
    }
    let canonical_roots = config
        .allowed_executable_roots
        .iter()
        .map(|root| {
            fs::canonicalize(root).map_err(|error| io_error("canonicalize executable root", error))
        })
        .collect::<RuntimeResult<Vec<_>>>()?;
    let invocation_parent = path
        .parent()
        .ok_or_else(|| RuntimeError::invalid("executable must have an absolute parent", field))?;
    let canonical_invocation_parent = fs::canonicalize(invocation_parent).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!("canonicalize executable invocation parent: {error}"),
            Some(field),
            false,
        )
    })?;
    let invocation_allowed = canonical_roots
        .iter()
        .any(|root| canonical_invocation_parent.starts_with(root));
    let target_allowed = canonical_roots
        .iter()
        .any(|root| canonical.starts_with(root));
    if !invocation_allowed || !target_allowed {
        return Err(RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            "executable invocation and target must both remain inside configured roots",
            Some(field),
            false,
        ));
    }
    // Preserve the caller-visible invocation path. The canonical target above owns
    // authority and digest validation; Runner executes that target with this path
    // restored as argv[0] so multicall/proxy executables retain their semantics.
    Ok(path.to_path_buf())
}

pub(super) fn validate_runner(path: &Path) -> RuntimeResult<PathBuf> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error("inspect Runner", error))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err(RuntimeError::invalid(
            "Runner must be a non-symlink executable file",
            "runnerPath",
        ));
    }
    fs::canonicalize(path).map_err(|error| io_error("canonicalize Runner", error))
}

pub(super) const CONTAINED_INPUT_ROOT: &str = "/run/ordivon/inputs";

pub(super) struct SystemdRunSpec<'a> {
    pub(super) unit_name: &'a str,
    pub(super) runner: &'a Path,
    pub(super) bundle_path: &'a Path,
    pub(super) workspace_path: &'a Path,
    pub(super) workspace_git_common_dir: Option<&'a Path>,
    pub(super) input_set_path: Option<&'a Path>,
    pub(super) runtime_ceiling_ms: u64,
    pub(super) budget: &'a super::ExecutionBudget,
    pub(super) execution_profile: super::ExecutionProfile,
    pub(super) environment: &'a BTreeMap<String, String>,
}

pub(super) fn build_systemd_run_command(spec: &SystemdRunSpec<'_>) -> RuntimeResult<Command> {
    let unit_name = spec.unit_name;
    let runner = spec.runner;
    let bundle_path = spec.bundle_path;
    let workspace_path = spec.workspace_path;
    let workspace_git_common_dir = spec.workspace_git_common_dir;
    let input_set_path = spec.input_set_path;
    let runtime_ceiling_ms = spec.runtime_ceiling_ms;
    let budget = spec.budget;
    let execution_profile = spec.execution_profile;
    let environment = spec.environment;
    let mut command = Command::new("systemd-run");
    command
        .arg(format!("--unit={unit_name}"))
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
        .arg(format!("--property=RuntimeMaxSec={runtime_ceiling_ms}ms"));
    if let Some(memory_max_bytes) = budget.memory_max_bytes {
        command.arg(format!("--property=MemoryMax={memory_max_bytes}"));
    }
    if let Some(tasks_max) = budget.tasks_max {
        command.arg(format!("--property=TasksMax={tasks_max}"));
    }
    if let Some(cpu_quota_percent) = budget.cpu_quota_percent {
        command.arg(format!("--property=CPUQuota={cpu_quota_percent}%"));
    }

    match execution_profile {
        crate::runtime::ExecutionProfile::TrustedLocal => {
            append_trusted_environment(&mut command);
            if let Some(input_set_path) = input_set_path {
                let source = systemd_path_value(input_set_path)?;
                command.arg(format!(
                    "--property=BindReadOnlyPaths={source}:{CONTAINED_INPUT_ROOT}"
                ));
            }
        }
        super::ExecutionProfile::ContainedLocal => {
            append_contained_properties(
                &mut command,
                runner,
                workspace_path,
                workspace_git_common_dir,
                input_set_path,
                bundle_path,
                environment,
            )?;
        }
    }

    command.arg(runner).arg("--task-dir").arg(bundle_path);
    Ok(command)
}

pub(super) fn append_contained_properties(
    command: &mut Command,
    runner: &Path,
    workspace_path: &Path,
    workspace_git_common_dir: Option<&Path>,
    input_set_path: Option<&Path>,
    bundle_path: &Path,
    environment: &BTreeMap<String, String>,
) -> RuntimeResult<()> {
    command.args([
        "--property=ProtectSystem=strict",
        "--property=ProtectHome=tmpfs",
        "--property=PrivateTmp=yes",
        "--property=PrivateDevices=yes",
        "--property=PrivateNetwork=yes",
        "--property=NoNewPrivileges=yes",
        "--property=CapabilityBoundingSet=",
        "--property=AmbientCapabilities=",
        "--property=RestrictSUIDSGID=yes",
        "--property=RestrictNamespaces=yes",
        "--property=LockPersonality=yes",
        "--property=ProtectHostname=yes",
        "--property=ProtectClock=yes",
        "--property=ProtectKernelTunables=yes",
        "--property=ProtectKernelModules=yes",
        "--property=ProtectKernelLogs=yes",
        "--property=ProtectControlGroups=yes",
        "--property=ProtectProc=invisible",
        "--property=ProcSubset=pid",
        "--property=KeyringMode=private",
        "--property=RestrictAddressFamilies=AF_UNIX",
        "--property=TemporaryFileSystem=/run:ro",
        "--property=TemporaryFileSystem=/var:ro",
        "--property=SystemCallFilter=~@mount @raw-io @reboot @swap @module @obsolete",
        "--property=UMask=0077",
    ]);

    let runner_value = systemd_path_value(runner)?;
    command.arg(format!(
        "--property=BindReadOnlyPaths={runner_value}:{runner_value}"
    ));
    if let Some(common_dir) = workspace_git_common_dir {
        let value = systemd_path_value(common_dir)?;
        command.arg(format!("--property=BindReadOnlyPaths={value}:{value}"));
    }
    if let Some(input_set_path) = input_set_path {
        let source = systemd_path_value(input_set_path)?;
        command.arg(format!(
            "--property=BindReadOnlyPaths={source}:{CONTAINED_INPUT_ROOT}"
        ));
    }
    for name in ["PATH", "HOME", "LANG", "LC_ALL", "TMPDIR", "XDG_CACHE_HOME"] {
        if let Some(value) = environment.get(name) {
            command.arg(format!("--setenv={name}={value}"));
        }
    }
    command.arg("--setenv=GIT_OPTIONAL_LOCKS=0");

    let mut writable_paths = std::collections::BTreeSet::new();
    writable_paths.insert(workspace_path.to_path_buf());
    writable_paths.insert(bundle_path.to_path_buf());
    for name in [
        "HOME",
        "TMPDIR",
        "XDG_CACHE_HOME",
        "CARGO_TARGET_DIR",
        "UV_CACHE_DIR",
        "PIP_CACHE_DIR",
        "npm_config_cache",
        "PNPM_HOME",
        "COREPACK_HOME",
        "BUN_INSTALL_CACHE_DIR",
        "GOMODCACHE",
        "GOCACHE",
    ] {
        if let Some(value) = environment.get(name) {
            writable_paths.insert(PathBuf::from(value));
        }
    }
    for path in writable_paths {
        let value = systemd_path_value(&path)?;
        command.arg(format!("--property=BindPaths={value}:{value}"));
    }
    Ok(())
}

pub(super) fn systemd_path_value(path: &Path) -> RuntimeResult<String> {
    if !path.is_absolute() {
        return Err(RuntimeError::invalid(
            "contained write paths must be absolute",
            "executionProfile",
        ));
    }
    let value = path.to_string_lossy();
    if value.is_empty() || value.as_bytes().contains(&0) || value.chars().any(char::is_whitespace) {
        return Err(RuntimeError::invalid(
            "contained write paths must be non-empty and whitespace-free",
            "executionProfile",
        ));
    }
    Ok(value.into_owned())
}

pub(super) fn append_trusted_environment(command: &mut Command) {
    for (name, value) in std::env::vars_os() {
        let Some(name) = name.to_str() else {
            continue;
        };
        if !valid_environment_name(name)
            || name.starts_with("ORDIVON_")
            || matches!(
                name,
                "INVOCATION_ID"
                    | "JOURNAL_STREAM"
                    | "LISTEN_FDS"
                    | "LISTEN_FDNAMES"
                    | "LISTEN_PID"
                    | "NOTIFY_SOCKET"
                    | "WATCHDOG_PID"
                    | "WATCHDOG_USEC"
            )
        {
            continue;
        }
        command.arg(format!("--setenv={name}={}", value.to_string_lossy()));
    }
}

pub(super) fn valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first == b'_' || first.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

pub(super) fn systemd_run(spec: &SystemdRunSpec<'_>) -> RuntimeResult<std::process::Output> {
    build_systemd_run_command(spec)?
        .output()
        .map_err(|error| tool_error("cannot execute systemd-run", error))
}

pub(super) fn release_terminal_unit(unit_name: &str) {
    // Failed transient units retain the supervisor evidence needed by recovery.
    // Only after Registry terminal commit is durable do we reset the failed state,
    // allowing systemd to unload the unit without an unbounded failed-unit leak.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let Ok(properties) = systemctl_show(unit_name) else {
            return;
        };
        if properties
            .get("LoadState")
            .is_some_and(|state| state == "not-found")
        {
            return;
        }
        if !unit_is_active(&properties) {
            let mut command = Command::new("systemctl");
            command.args(["reset-failed", unit_name]);
            let _ = bounded_observation_output(
                &mut command,
                SYSTEMCTL_OBSERVATION_TIMEOUT,
                "systemctl reset-failed",
            );
            return;
        }
        if Instant::now() >= deadline {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn bounded_observation_output(
    command: &mut Command,
    timeout: Duration,
    context: &str,
) -> RuntimeResult<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| tool_error(&format!("cannot execute {context}"), error))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child
            .try_wait()
            .map_err(|error| tool_error(&format!("cannot observe {context}"), error))?
        {
            Some(_) => {
                return child
                    .wait_with_output()
                    .map_err(|error| tool_error(&format!("cannot collect {context}"), error));
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RuntimeError::new(
                    RuntimeErrorCode::ToolFailed,
                    format!("{context} timed out after {} ms", timeout.as_millis()),
                    None,
                    true,
                ));
            }
            None => thread::sleep(OBSERVATION_COMMAND_POLL.min(timeout)),
        }
    }
}

pub(super) fn systemctl_show(unit_name: &str) -> RuntimeResult<BTreeMap<String, String>> {
    let mut command = Command::new("systemctl");
    command.args([
        "show",
        unit_name,
        "--property=LoadState,ActiveState,SubState,Job,InvocationID,ControlGroup,MainPID,Result,ExecMainCode,ExecMainStatus",
    ]);
    let output = bounded_observation_output(
        &mut command,
        SYSTEMCTL_OBSERVATION_TIMEOUT,
        "systemctl show",
    )?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ToolFailed,
            format!(
                "systemctl show failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            None,
            true,
        ));
    }
    let mut properties = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            properties.insert(key.to_string(), value.to_string());
        }
    }
    properties
        .entry("LoadState".to_string())
        .or_insert_with(|| "not-found".to_string());
    properties
        .entry("ActiveState".to_string())
        .or_insert_with(|| "inactive".to_string());
    Ok(properties)
}

pub(super) fn unit_is_active(properties: &BTreeMap<String, String>) -> bool {
    properties
        .get("ActiveState")
        .is_some_and(|state| matches!(state.as_str(), "active" | "activating" | "reloading"))
}

/// A non-empty Unit.Job means systemd still owns an in-flight manager job for this
/// exact transient unit. With `systemd-run --no-block`, enqueueing is the physical
/// dispatch boundary; absence of Runner start evidence while this job exists is not
/// evidence that dispatch was lost.
pub(super) fn unit_has_pending_job(properties: &BTreeMap<String, String>) -> bool {
    properties.get("Job").is_some_and(|job| !job.is_empty())
}

pub(super) fn nonempty_property(
    properties: &BTreeMap<String, String>,
    key: &str,
) -> Option<String> {
    properties
        .get(key)
        .filter(|value| !value.is_empty())
        .cloned()
}

pub(super) fn require_property(
    properties: &BTreeMap<String, String>,
    key: &str,
    expected: &str,
) -> RuntimeResult<()> {
    if properties.get(key).map(String::as_str) != Some(expected) {
        return Err(RuntimeError::new(
            RuntimeErrorCode::LaunchIdentityMismatch,
            format!("systemd {key} does not match runner-start evidence"),
            Some(key),
            false,
        ));
    }
    Ok(())
}

pub(super) fn missing_systemd_property(key: &str) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::LaunchIdentityMismatch,
        format!("systemd omitted {key}"),
        Some(key),
        false,
    )
}

pub(super) fn supervisor_identity(attempt: &AttemptRecord) -> RuntimeResult<SupervisorIdentity> {
    Ok(SupervisorIdentity {
        boot_id: attempt.boot_id.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no bootId",
                Some("bootId"),
                false,
            )
        })?,
        unit_name: attempt.unit_name.clone(),
        invocation_id: attempt.invocation_id.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no invocationId",
                Some("invocationId"),
                false,
            )
        })?,
        control_group: attempt.control_group.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no controlGroup",
                Some("controlGroup"),
                false,
            )
        })?,
        main_pid: attempt.main_pid.ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no mainPid",
                Some("mainPid"),
                false,
            )
        })?,
        main_process_start_identity: attempt.process_start_identity.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no process start identity",
                Some("processStartIdentity"),
                false,
            )
        })?,
    })
}

pub(super) fn cgroup_has_processes(control_group: &str) -> RuntimeResult<bool> {
    if !control_group.starts_with('/')
        || control_group
            .split('/')
            .any(|part| part == ".." || part.contains('\0'))
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            "recorded cgroup path is invalid",
            Some("controlGroup"),
            false,
        ));
    }
    let root = Path::new("/sys/fs/cgroup").join(control_group.trim_start_matches('/'));
    let events_path = root.join("cgroup.events");
    if events_path.is_file() {
        let content = fs::read_to_string(events_path)
            .map_err(|error| io_error("read cgroup population state", error))?;
        return parse_cgroup_populated(&content);
    }

    let processes_path = root.join("cgroup.procs");
    if !processes_path.is_file() {
        return Ok(false);
    }
    let content = fs::read_to_string(processes_path)
        .map_err(|error| io_error("read cgroup process membership", error))?;
    Ok(content
        .lines()
        .any(|line| line.trim().parse::<u32>().is_ok()))
}

pub(super) fn parse_cgroup_populated(content: &str) -> RuntimeResult<bool> {
    for line in content.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() == Some("populated") {
            return match fields.next() {
                Some("0") if fields.next().is_none() => Ok(false),
                Some("1") if fields.next().is_none() => Ok(true),
                _ => Err(RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "cgroup.events has an invalid populated value",
                    Some("controlGroup"),
                    false,
                )),
            };
        }
    }
    Err(RuntimeError::new(
        RuntimeErrorCode::RegistryCorrupt,
        "cgroup.events omitted populated state",
        Some("controlGroup"),
        false,
    ))
}

pub(super) fn process_identity(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = stat.rfind(')')?;
    stat[close + 1..]
        .split_whitespace()
        .nth(19)
        .map(ToString::to_string)
}

pub(super) fn read_trimmed(path: &str) -> RuntimeResult<String> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|error| io_error(&format!("read {path}"), error))
}

fn io_error(context: &str, error: std::io::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::IoError,
        format!("{context}: {error}"),
        None,
        error.kind() == std::io::ErrorKind::Interrupted,
    )
}

fn tool_error(context: &str, error: std::io::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::ToolFailed,
        format!("{context}: {error}"),
        None,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_validation_preserves_absolute_proxy_invocation_path() {
        let root =
            std::env::temp_dir().join(format!("ordivon-executable-proxy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let proxy = root.join("true-proxy");
        std::os::unix::fs::symlink("/usr/bin/true", &proxy).unwrap();
        let config = UniversalExecutorConfig {
            store_root: root.join("store"),
            workspace_root: None,
            workspace_uid: None,
            workspace_gid: None,
            runner_path: PathBuf::from("/usr/bin/true"),
            allowed_executable_roots: vec![root.clone(), PathBuf::from("/usr/bin")],
            max_runtime_ms: 60_000,
            max_output_bytes: 1_048_576,
        };
        assert_eq!(
            validate_executable(&config, proxy.to_str().unwrap(), "execution.executable").unwrap(),
            proxy
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn executable_validation_rejects_proxy_invocation_outside_allowed_roots() {
        let root = std::env::temp_dir().join(format!(
            "ordivon-executable-proxy-outside-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let proxy = root.join("true-proxy");
        std::os::unix::fs::symlink("/usr/bin/true", &proxy).unwrap();
        let config = UniversalExecutorConfig {
            store_root: root.join("store"),
            workspace_root: None,
            workspace_uid: None,
            workspace_gid: None,
            runner_path: PathBuf::from("/usr/bin/true"),
            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
            max_runtime_ms: 60_000,
            max_output_bytes: 1_048_576,
        };
        let error = validate_executable(&config, proxy.to_str().unwrap(), "execution.executable")
            .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
        assert_eq!(error.field.as_deref(), Some("execution.executable"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn executable_validation_rejects_relative_paths_before_resolution() {
        let config = UniversalExecutorConfig {
            store_root: PathBuf::from("/tmp/ordivon-relative-executable-test"),
            workspace_root: None,
            workspace_uid: None,
            workspace_gid: None,
            runner_path: PathBuf::from("/usr/bin/true"),
            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
            max_runtime_ms: 60_000,
            max_output_bytes: 1_048_576,
        };
        let error = validate_executable(&config, "true", "execution.executable").unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
        assert_eq!(error.field.as_deref(), Some("execution.executable"));
    }

    #[test]
    fn executable_validation_preserves_exact_step_field_on_physical_failure() {
        let config = UniversalExecutorConfig {
            store_root: PathBuf::from("/tmp/ordivon-missing-executable-test"),
            workspace_root: None,
            workspace_uid: None,
            workspace_gid: None,
            runner_path: PathBuf::from("/usr/bin/true"),
            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
            max_runtime_ms: 60_000,
            max_output_bytes: 1_048_576,
        };
        let error = validate_executable(
            &config,
            "/definitely/not/an/ordivon/executable",
            "execution.steps[3].executable",
        )
        .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::IoError);
        assert_eq!(
            error.field.as_deref(),
            Some("execution.steps[3].executable")
        );
    }

    #[test]
    fn systemd_run_command_submits_without_waiting_for_start_job_completion() {
        let budget = super::super::ExecutionBudget::default();
        let environment = BTreeMap::new();
        let command = build_systemd_run_command(&SystemdRunSpec {
            unit_name: "ordivon-test.service",
            runner: Path::new("/usr/bin/true"),
            bundle_path: Path::new("/tmp/ordivon-test-bundle"),
            workspace_path: Path::new("/tmp/ordivon-test-workspace"),
            workspace_git_common_dir: None,
            input_set_path: None,
            runtime_ceiling_ms: 5_000,
            budget: &budget,
            execution_profile: super::super::ExecutionProfile::TrustedLocal,
            environment: &environment,
        })
        .unwrap();
        assert!(command.get_args().any(|arg| arg == "--no-block"));
    }

    #[test]
    fn pending_manager_job_is_distinguished_from_settled_inactive_unit() {
        let pending = BTreeMap::from([
            ("ActiveState".to_string(), "inactive".to_string()),
            ("Job".to_string(), "4242".to_string()),
        ]);
        assert!(!unit_is_active(&pending));
        assert!(unit_has_pending_job(&pending));

        let settled = BTreeMap::from([
            ("ActiveState".to_string(), "inactive".to_string()),
            ("Job".to_string(), String::new()),
        ]);
        assert!(!unit_is_active(&settled));
        assert!(!unit_has_pending_job(&settled));
    }

    #[test]
    fn bounded_observation_output_fails_fast_for_hung_cli() {
        let mut command = Command::new("/usr/bin/sh");
        command.args(["-c", "sleep 5"]);
        let started = Instant::now();
        let error =
            bounded_observation_output(&mut command, Duration::from_millis(50), "test observation")
                .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::ToolFailed);
        assert!(error.retryable);
        assert!(error.message.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn bounded_observation_output_preserves_success_output() {
        let mut command = Command::new("/usr/bin/printf");
        command.arg("ok");
        let output =
            bounded_observation_output(&mut command, Duration::from_secs(1), "test observation")
                .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"ok");
    }
}
