use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::migration::{
    ArtifactKind, ArtifactReference, MigrationBackend, MigrationTaskHandle, MigrationTaskStatus,
    MIGRATION_CONTRACT_SCHEMA_VERSION,
};

use super::super::{
    now_unix_ms, sha256_file, validate_id, write_json_atomic, CapturedOutput, RunnerTaskResult,
    TaskCancelRequest, TaskGetRequest, TaskMetadata, TaskTerminalStatus, UniversalExecError,
    UniversalExecErrorCode, UniversalExecutorConfig, UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use super::{CANCEL_FILE, METADATA_FILE, RESULT_FILE, STDERR_FILE, STDOUT_FILE};

pub fn get_universal_task(
    config: &UniversalExecutorConfig,
    request: &TaskGetRequest,
) -> Result<MigrationTaskHandle, UniversalExecError> {
    request.validate_shape()?;
    let task_dir = config.task_path(&request.task_id);
    let metadata = load_task_metadata(&task_dir, &request.task_id)?;
    let deadline = Instant::now() + Duration::from_millis(request.wait_ms);
    loop {
        if let Some(result) = load_task_result_if_present(&task_dir)? {
            return task_handle_from_result(&task_dir, &metadata, &result);
        }
        let properties = systemctl_show(&metadata.unit_name)?;
        let active = properties
            .get("ActiveState")
            .map(String::as_str)
            .unwrap_or("unknown");
        if matches!(active, "active" | "activating" | "reloading") {
            if request.wait_ms > 0 && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            return Ok(MigrationTaskHandle {
                schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
                task_id: request.task_id.clone(),
                backend: MigrationBackend::Ordivon,
                status: MigrationTaskStatus::Working,
                status_message: format!(
                    "Task is {} (substate {}).",
                    active,
                    properties
                        .get("SubState")
                        .map(String::as_str)
                        .unwrap_or("unknown")
                ),
                result_available: false,
                poll_after_ms: Some(250),
                event_cursor: properties
                    .get("InvocationID")
                    .filter(|value| !value.is_empty())
                    .cloned(),
                required_input: None,
                artifacts: Vec::new(),
            });
        }
        if request.wait_ms > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::TaskStateUnavailable,
            format!(
                "task unit is {active} but no durable result is present; retry after a short delay"
            ),
            Some("taskId"),
            true,
        ));
    }
}
pub fn cancel_universal_task(
    config: &UniversalExecutorConfig,
    request: &TaskCancelRequest,
) -> Result<MigrationTaskHandle, UniversalExecError> {
    request.validate_shape()?;
    let task_dir = config.task_path(&request.task_id);
    let metadata = load_task_metadata(&task_dir, &request.task_id)?;
    if let Some(result) = load_task_result_if_present(&task_dir)? {
        return task_handle_from_result(&task_dir, &metadata, &result);
    }
    write_json_atomic(
        &task_dir.join(CANCEL_FILE),
        &serde_json::json!({
            "schemaVersion": UNIVERSAL_EXEC_SCHEMA_VERSION,
            "taskId": request.task_id,
            "requestedUnixMs": now_unix_ms()?
        }),
    )?;
    let output = Command::new("systemctl")
        .args(["stop", &metadata.unit_name])
        .output()
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolUnavailable,
                format!("cannot execute systemctl stop: {error}"),
                None,
                false,
            )
        })?;
    if !output.status.success() {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!(
                "systemctl stop failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            None,
            true,
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if let Some(result) = load_task_result_if_present(&task_dir)? {
            return task_handle_from_result(&task_dir, &metadata, &result);
        }
        thread::sleep(Duration::from_millis(20));
    }
    let result = RunnerTaskResult {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: request.task_id.clone(),
        job_id: None,
        attempt_id: None,
        launch_token_digest: None,
        payload_uid: None,
        payload_gid: None,
        status: TaskTerminalStatus::Cancelled,
        exit_code: None,
        timed_out: false,
        infrastructure_error: None,
        started_unix_ms: metadata.created_unix_ms,
        finished_unix_ms: now_unix_ms()?,
        stdout: capture_existing_output(&task_dir, &request.task_id, true)?,
        stderr: capture_existing_output(&task_dir, &request.task_id, false)?,
    };
    write_json_atomic(&task_dir.join(RESULT_FILE), &result)?;
    task_handle_from_result(&task_dir, &metadata, &result)
}
pub(super) fn load_task_metadata(
    task_dir: &Path,
    task_id: &str,
) -> Result<TaskMetadata, UniversalExecError> {
    validate_id(task_id, "taskId")?;
    let bytes = fs::read(task_dir.join(METADATA_FILE)).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::TaskNotFound,
            format!("cannot read task metadata: {error}"),
            Some("taskId"),
            false,
        )
    })?;
    let metadata: TaskMetadata = serde_json::from_slice(&bytes).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("invalid task metadata: {error}"),
            Some("taskId"),
            false,
        )
    })?;
    if metadata.task_id != task_id {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            "task metadata identity mismatch",
            Some("taskId"),
            false,
        ));
    }
    Ok(metadata)
}
pub(super) fn load_task_result_if_present(
    task_dir: &Path,
) -> Result<Option<RunnerTaskResult>, UniversalExecError> {
    let path = task_dir.join(RESULT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("cannot read task result: {error}"),
            None,
            false,
        )
    })?;
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("invalid task result: {error}"),
            None,
            false,
        )
    })
}
pub(super) fn task_handle_from_result(
    task_dir: &Path,
    metadata: &TaskMetadata,
    result: &RunnerTaskResult,
) -> Result<MigrationTaskHandle, UniversalExecError> {
    if result.task_id != metadata.task_id {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            "task result identity mismatch",
            Some("taskId"),
            false,
        ));
    }
    let (status, message) = match result.status {
        TaskTerminalStatus::Completed => (
            MigrationTaskStatus::Completed,
            "Task completed successfully.".to_string(),
        ),
        TaskTerminalStatus::Failed if result.timed_out => (
            MigrationTaskStatus::Failed,
            "Task exceeded its runtime limit.".to_string(),
        ),
        TaskTerminalStatus::Failed => (
            MigrationTaskStatus::Failed,
            result
                .infrastructure_error
                .clone()
                .unwrap_or_else(|| format!("Task failed with exit code {:?}.", result.exit_code)),
        ),
        TaskTerminalStatus::Cancelled => (
            MigrationTaskStatus::Cancelled,
            "Task was cancelled.".to_string(),
        ),
    };
    let result_path = task_dir.join(RESULT_FILE);
    let artifacts = vec![
        output_artifact(metadata, &result.stdout, ArtifactKind::Stdout),
        output_artifact(metadata, &result.stderr, ArtifactKind::Stderr),
        ArtifactReference {
            artifact_id: format!("{}.result", metadata.task_id),
            task_id: metadata.task_id.clone(),
            kind: ArtifactKind::ExecutionResult,
            digest: sha256_file(&result_path)?,
            media_type: "application/json".to_string(),
            byte_length: fs::metadata(&result_path)
                .map_err(|error| {
                    UniversalExecError::new(
                        UniversalExecErrorCode::IoError,
                        format!("cannot inspect task result: {error}"),
                        None,
                        false,
                    )
                })?
                .len(),
        },
    ];
    let handle = MigrationTaskHandle {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        task_id: metadata.task_id.clone(),
        backend: MigrationBackend::Ordivon,
        status,
        status_message: message,
        result_available: true,
        poll_after_ms: None,
        event_cursor: Some(format!("terminal-{}", result.finished_unix_ms)),
        required_input: None,
        artifacts,
    };
    handle.validate_shape().map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("invalid migrated task handle: {error}"),
            None,
            false,
        )
    })?;
    Ok(handle)
}
fn output_artifact(
    metadata: &TaskMetadata,
    output: &CapturedOutput,
    kind: ArtifactKind,
) -> ArtifactReference {
    ArtifactReference {
        artifact_id: output.artifact_id.clone(),
        task_id: metadata.task_id.clone(),
        kind,
        digest: output.digest.clone(),
        media_type: "text/plain; charset=utf-8".to_string(),
        byte_length: output.retained_bytes,
    }
}
pub(super) fn capture_existing_output(
    task_dir: &Path,
    task_id: &str,
    stdout: bool,
) -> Result<CapturedOutput, UniversalExecError> {
    let (suffix, file_name) = if stdout {
        ("stdout", STDOUT_FILE)
    } else {
        ("stderr", STDERR_FILE)
    };
    let path = task_dir.join(file_name);
    if !path.exists() {
        fs::write(&path, []).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot create output artifact: {error}"),
                None,
                false,
            )
        })?;
    }
    let size = fs::metadata(&path)
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot inspect output artifact: {error}"),
                None,
                false,
            )
        })?
        .len();
    Ok(CapturedOutput {
        artifact_id: format!("{task_id}.{suffix}"),
        file_name: file_name.to_string(),
        digest: sha256_file(&path)?,
        retained_bytes: size,
        dropped_bytes: 0,
        truncated: false,
    })
}
fn systemctl_show(
    unit: &str,
) -> Result<std::collections::BTreeMap<String, String>, UniversalExecError> {
    let output = Command::new("systemctl")
        .arg("show")
        .arg(unit)
        .args([
            "-pLoadState",
            "-pActiveState",
            "-pSubState",
            "-pResult",
            "-pInvocationID",
            "-pMainPID",
        ])
        .output()
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolUnavailable,
                format!("cannot execute systemctl show: {error}"),
                None,
                true,
            )
        })?;
    if !output.status.success() {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!(
                "systemctl show failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            None,
            true,
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect())
}
