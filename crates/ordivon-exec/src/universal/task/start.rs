use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::migration::{
    MigrationBackend, MigrationTaskHandle, MigrationTaskStatus, MIGRATION_CONTRACT_SCHEMA_VERSION,
};

use super::super::{
    canonical_directory, invalid, load_workspace_record, now_unix_ms, resolve_workspace_cwd,
    sha256_bytes, sha256_file, write_json_atomic, RunnerTaskRequest, RunnerTaskResult,
    TaskMetadata, TaskTerminalStatus, UniversalExecError, UniversalExecErrorCode,
    UniversalExecRequest, UniversalExecutorConfig, UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use super::status::{capture_existing_output, task_handle_from_result};
use super::{METADATA_FILE, REQUEST_FILE, RESULT_FILE};

pub fn start_universal_task(
    config: &UniversalExecutorConfig,
    request: &UniversalExecRequest,
) -> Result<MigrationTaskHandle, UniversalExecError> {
    config.ensure_store()?;
    request.validate_shape()?;
    if request.timeout_ms > config.max_runtime_ms {
        return Err(invalid(
            "timeoutMs exceeds executor configuration",
            "timeoutMs",
        ));
    }
    if request.stdout_limit_bytes > config.max_output_bytes
        || request.stderr_limit_bytes > config.max_output_bytes
    {
        return Err(invalid(
            "output limit exceeds executor configuration",
            "stdoutLimitBytes",
        ));
    }
    let record = load_workspace_record(config, &request.workspace_id)?;
    let workspace_path = canonical_directory(Path::new(&record.workspace_path), "workspacePath")?;
    let cwd = resolve_workspace_cwd(&record, &request.cwd_relative)?;
    let executable = validate_executable(config, Path::new(&request.executable))?;
    let runner = validate_runner(&config.runner_path)?;
    let task_dir = config.task_path(&request.task_id);
    if task_dir.exists() {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::TaskExists,
            "task already exists",
            Some("taskId"),
            false,
        ));
    }
    fs::create_dir(&task_dir).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("cannot create task directory: {error}"),
            Some("taskId"),
            false,
        )
    })?;
    let runner_request = RunnerTaskRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: request.task_id.clone(),
        workspace_id: request.workspace_id.clone(),
        workspace_path: workspace_path.to_string_lossy().into_owned(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable)?,
        args: request.args.clone(),
        cwd: cwd.to_string_lossy().into_owned(),
        env: request.env.clone(),
        timeout_ms: request.timeout_ms,
        stdout_limit_bytes: request.stdout_limit_bytes,
        stderr_limit_bytes: request.stderr_limit_bytes,
    };
    let request_bytes = serde_json::to_vec(&runner_request).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("cannot serialize task request: {error}"),
            None,
            false,
        )
    })?;
    let unit_name = format!("ordivon-m1-{}.service", request.task_id);
    let metadata = TaskMetadata {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: request.task_id.clone(),
        workspace_id: request.workspace_id.clone(),
        unit_name: unit_name.clone(),
        request_digest: sha256_bytes(&request_bytes),
        boot_id: read_boot_id()?,
        created_unix_ms: now_unix_ms()?,
    };
    write_json_atomic(&task_dir.join(REQUEST_FILE), &runner_request)?;
    write_json_atomic(&task_dir.join(METADATA_FILE), &metadata)?;

    let runtime_ceiling = request.timeout_ms.saturating_add(5_000);
    let output = Command::new("systemd-run")
        .arg(format!("--unit={unit_name}"))
        .arg("--collect")
        .args([
            "--property=Type=exec",
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=2s",
            "--property=SendSIGKILL=yes",
            "--property=NoNewPrivileges=yes",
            "--property=CapabilityBoundingSet=",
            "--property=AmbientCapabilities=",
            "--property=ProtectSystem=strict",
            "--property=PrivateTmp=yes",
            "--property=PrivateNetwork=yes",
            "--property=PrivateDevices=yes",
            "--property=PrivateIPC=yes",
            "--property=PrivatePIDs=yes",
            "--property=ProtectProc=invisible",
            "--property=ProcSubset=pid",
            "--property=RestrictNamespaces=yes",
            "--property=RestrictAddressFamilies=AF_UNIX",
            "--property=ProtectKernelTunables=yes",
            "--property=ProtectKernelModules=yes",
            "--property=ProtectControlGroups=yes",
            "--property=ProtectHostname=yes",
            "--property=ProtectClock=yes",
            "--property=RestrictSUIDSGID=yes",
            "--property=LockPersonality=yes",
            "--property=SystemCallArchitectures=native",
            "--property=InaccessiblePaths=-/run/systemd/private -/run/dbus/system_bus_socket -/run/docker.sock -/var/run/docker.sock -/run/credentials -/root/.ssh -/root/.cloudflared -/root/.config -/root/.aws -/root/.kube -/root/.docker -/root/.git-credentials -/root/.netrc",
            "--property=UMask=0077",
            "--property=TasksMax=128",
            "--property=MemoryMax=1073741824",
            "--property=StandardOutput=journal",
            "--property=StandardError=journal",
        ])
        .arg(format!("--property=RuntimeMaxSec={runtime_ceiling}ms"))
        .arg(format!(
            "--property=ReadWritePaths={} {}",
            workspace_path.display(),
            task_dir.display()
        ))
        .arg(&runner)
        .arg("--task-dir")
        .arg(&task_dir)
        .output()
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolUnavailable,
                format!("cannot execute systemd-run: {error}"),
                None,
                false,
            )
        })?;
    if !output.status.success() {
        let result = infrastructure_failure_result(
            &request.task_id,
            metadata.created_unix_ms,
            format!(
                "systemd-run failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            &task_dir,
        )?;
        write_json_atomic(&task_dir.join(RESULT_FILE), &result)?;
        return task_handle_from_result(&task_dir, &metadata, &result);
    }
    Ok(MigrationTaskHandle {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        task_id: request.task_id.clone(),
        backend: MigrationBackend::Ordivon,
        status: MigrationTaskStatus::Working,
        status_message: "Task launched in a transient systemd service.".to_string(),
        result_available: false,
        poll_after_ms: Some(250),
        event_cursor: Some(format!("created-{}", metadata.created_unix_ms)),
        required_input: None,
        artifacts: Vec::new(),
    })
}
fn validate_executable(
    config: &UniversalExecutorConfig,
    path: &Path,
) -> Result<PathBuf, UniversalExecError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            format!("cannot inspect executable: {error}"),
            Some("executable"),
            false,
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid(
            "executable must be a non-symlink regular file",
            "executable",
        ));
    }
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(invalid(
            "executable has no execute permission",
            "executable",
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            format!("cannot canonicalize executable: {error}"),
            Some("executable"),
            false,
        )
    })?;
    let allowed = config.allowed_executable_roots.iter().any(|root| {
        fs::canonicalize(root)
            .map(|allowed_root| canonical.starts_with(allowed_root))
            .unwrap_or(false)
    });
    if !allowed {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspacePathDenied,
            "executable is outside configured roots",
            Some("executable"),
            false,
        ));
    }
    Ok(canonical)
}
fn validate_runner(path: &Path) -> Result<PathBuf, UniversalExecError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            format!("cannot inspect runner: {error}"),
            Some("runnerPath"),
            false,
        )
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err(invalid(
            "runner must be a non-symlink executable file",
            "runnerPath",
        ));
    }
    fs::canonicalize(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            format!("cannot canonicalize runner: {error}"),
            Some("runnerPath"),
            false,
        )
    })
}
fn infrastructure_failure_result(
    task_id: &str,
    started_unix_ms: u128,
    error: String,
    task_dir: &Path,
) -> Result<RunnerTaskResult, UniversalExecError> {
    Ok(RunnerTaskResult {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: task_id.to_string(),
        status: TaskTerminalStatus::Failed,
        exit_code: None,
        timed_out: false,
        infrastructure_error: Some(error),
        started_unix_ms,
        finished_unix_ms: now_unix_ms()?,
        stdout: capture_existing_output(task_dir, task_id, true)?,
        stderr: capture_existing_output(task_dir, task_id, false)?,
    })
}
fn read_boot_id() -> Result<String, UniversalExecError> {
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map(|value| value.trim().to_string())
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot read kernel boot ID: {error}"),
                None,
                false,
            )
        })
}
