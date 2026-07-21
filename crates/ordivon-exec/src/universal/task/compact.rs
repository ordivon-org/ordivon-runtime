use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::migration::{MigrationTaskHandle, MigrationTaskStatus};

use super::super::{
    CompactTaskObservation, DurableTaskSnapshot, RunnerTaskResult, TaskAwaitRequest,
    TaskGetRequest, TaskRunRequest, TaskTerminalStatus, UniversalExecError,
    UniversalExecutorConfig,
};
use super::start::start_universal_task;
use super::status::{get_universal_task, load_task_metadata, load_task_result_if_present};
use super::{STDERR_FILE, STDOUT_FILE};

pub fn snapshot_universal_task(
    config: &UniversalExecutorConfig,
    task_id: &str,
) -> Result<DurableTaskSnapshot, UniversalExecError> {
    let task_dir = config.task_path(task_id);
    let metadata = load_task_metadata(&task_dir, task_id)?;
    let handle = get_universal_task(
        config,
        &TaskGetRequest {
            schema_version: super::super::UNIVERSAL_EXEC_SCHEMA_VERSION,
            task_id: task_id.to_string(),
            wait_ms: 0,
        },
    )?;
    let updated_unix_ms = load_task_result_if_present(&task_dir)?
        .map(|result| result.finished_unix_ms)
        .unwrap_or(metadata.created_unix_ms);
    Ok(DurableTaskSnapshot {
        task_id: handle.task_id,
        status: handle.status,
        status_message: handle.status_message,
        created_unix_ms: metadata.created_unix_ms,
        updated_unix_ms,
        poll_after_ms: handle.poll_after_ms,
        result_available: handle.result_available,
    })
}

pub fn run_universal_task_compact(
    config: &UniversalExecutorConfig,
    request: &TaskRunRequest,
) -> Result<CompactTaskObservation, UniversalExecError> {
    request.validate_shape()?;
    let started = start_universal_task(config, &request.execution)?;
    if started.status != MigrationTaskStatus::Working {
        return compact_terminal_from_store(
            config,
            &request.execution.task_id,
            request.stdout_tail_bytes,
            request.stderr_tail_bytes,
        );
    }
    await_universal_task_compact(
        config,
        &TaskAwaitRequest {
            schema_version: request.schema_version,
            task_id: request.execution.task_id.clone(),
            wait_ms: request.wait_ms,
            stdout_tail_bytes: request.stdout_tail_bytes,
            stderr_tail_bytes: request.stderr_tail_bytes,
        },
    )
}

pub fn await_universal_task_compact(
    config: &UniversalExecutorConfig,
    request: &TaskAwaitRequest,
) -> Result<CompactTaskObservation, UniversalExecError> {
    request.validate_shape()?;
    let handle = get_universal_task(
        config,
        &TaskGetRequest {
            schema_version: request.schema_version,
            task_id: request.task_id.clone(),
            wait_ms: request.wait_ms,
        },
    )?;
    if handle.status == MigrationTaskStatus::Working {
        return Ok(compact_working(handle));
    }
    compact_terminal_from_store(
        config,
        &request.task_id,
        request.stdout_tail_bytes,
        request.stderr_tail_bytes,
    )
}

fn compact_working(handle: MigrationTaskHandle) -> CompactTaskObservation {
    CompactTaskObservation {
        task_id: handle.task_id,
        status: MigrationTaskStatus::Working,
        exit_code: None,
        timed_out: false,
        poll_after_ms: handle.poll_after_ms,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        artifacts_available: false,
        error_summary: None,
    }
}

fn compact_terminal_from_store(
    config: &UniversalExecutorConfig,
    task_id: &str,
    stdout_tail_bytes: u64,
    stderr_tail_bytes: u64,
) -> Result<CompactTaskObservation, UniversalExecError> {
    let task_dir = config.task_path(task_id);
    let metadata = load_task_metadata(&task_dir, task_id)?;
    let result = load_task_result_if_present(&task_dir)?.ok_or_else(|| {
        UniversalExecError::new(
            super::super::UniversalExecErrorCode::TaskStateUnavailable,
            "terminal task has no durable result",
            Some("taskId"),
            true,
        )
    })?;
    if result.task_id != metadata.task_id {
        return Err(UniversalExecError::new(
            super::super::UniversalExecErrorCode::MetadataCorrupt,
            "task result identity mismatch",
            Some("taskId"),
            false,
        ));
    }
    observation_from_result(&task_dir, &result, stdout_tail_bytes, stderr_tail_bytes)
}

fn observation_from_result(
    task_dir: &Path,
    result: &RunnerTaskResult,
    stdout_tail_bytes: u64,
    stderr_tail_bytes: u64,
) -> Result<CompactTaskObservation, UniversalExecError> {
    let (status, error_summary) = match result.status {
        TaskTerminalStatus::Completed => (MigrationTaskStatus::Completed, None),
        TaskTerminalStatus::Cancelled => (MigrationTaskStatus::Cancelled, None),
        TaskTerminalStatus::Failed => (
            MigrationTaskStatus::Failed,
            result.infrastructure_error.clone().or_else(|| {
                result
                    .exit_code
                    .map(|code| format!("process exited with code {code}"))
            }),
        ),
    };
    let (stdout_tail, stdout_tail_cut) = read_tail(&task_dir.join(STDOUT_FILE), stdout_tail_bytes)?;
    let (stderr_tail, stderr_tail_cut) = read_tail(&task_dir.join(STDERR_FILE), stderr_tail_bytes)?;
    Ok(CompactTaskObservation {
        task_id: result.task_id.clone(),
        status,
        exit_code: result.exit_code,
        timed_out: result.timed_out,
        poll_after_ms: None,
        stdout_tail,
        stderr_tail,
        stdout_truncated: result.stdout.truncated || stdout_tail_cut,
        stderr_truncated: result.stderr.truncated || stderr_tail_cut,
        artifacts_available: true,
        error_summary,
    })
}

fn read_tail(path: &Path, max_bytes: u64) -> Result<(String, bool), UniversalExecError> {
    if max_bytes == 0 {
        let length = std::fs::metadata(path)
            .map_err(|error| super::super::io_error(path, "inspect", error))?
            .len();
        return Ok((String::new(), length > 0));
    }
    let mut file = File::open(path).map_err(|error| super::super::io_error(path, "open", error))?;
    let length = file
        .metadata()
        .map_err(|error| super::super::io_error(path, "inspect", error))?
        .len();
    let start = length.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| super::super::io_error(path, "seek", error))?;
    let mut bytes = Vec::with_capacity((length - start) as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| super::super::io_error(path, "read", error))?;
    Ok((String::from_utf8_lossy(&bytes).into_owned(), start > 0))
}
