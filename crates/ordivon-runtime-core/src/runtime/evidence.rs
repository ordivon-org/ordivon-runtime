use std::fs;
use std::path::Path;

use super::{
    ArtifactRegistration, AttemptRecord, AttemptState, AttemptTerminationIntent, RuntimeError,
    RuntimeErrorCode, RuntimeResult, TerminalCommit,
};
use crate::universal::{
    sha256_bytes, sha256_file, CapturedOutput, RunnerTaskResult, TaskTerminalStatus,
};

pub(crate) const RESULT_FILE: &str = "result.json";
const STDOUT_FILE: &str = "stdout.log";
const STDERR_FILE: &str = "stderr.log";

pub(crate) fn prepare_runner_terminal_from_bundle(
    current: &AttemptRecord,
) -> RuntimeResult<TerminalCommit> {
    let result_path = Path::new(&current.bundle_path).join(RESULT_FILE);
    let bytes = fs::read(&result_path).map_err(|error| io_error("read Runner result", error))?;
    let result: RunnerTaskResult = serde_json::from_slice(&bytes).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("invalid Runner result: {error}"),
            Some("result"),
            false,
        )
    })?;
    if result.task_id != current.attempt_id
        || result.job_id.as_deref() != Some(current.job_id.as_str())
        || result.attempt_id.as_deref() != Some(current.attempt_id.as_str())
        || result.launch_token_digest.as_deref() != Some(current.launch_token_digest.as_str())
        || result.payload_uid.is_some()
        || result.payload_gid.is_some()
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ResultIdentityConflict,
            "Runner result identity does not match committed Attempt",
            Some("result"),
            false,
        ));
    }
    let result_digest = sha256_bytes(&bytes);
    let stdout = validate_captured_output(current, &result.stdout, true)?;
    let stderr = validate_captured_output(current, &result.stderr, false)?;
    let (state, reason_code) = match result.status {
        TaskTerminalStatus::Completed
            if current.state == AttemptState::Stopping
                || current.termination_intent == AttemptTerminationIntent::StopRequested =>
        {
            (
                AttemptState::Succeeded,
                "PROCESS_COMPLETED_BEFORE_STOP_EFFECTIVE",
            )
        }
        TaskTerminalStatus::Completed => (AttemptState::Succeeded, "PROCESS_EXIT_ZERO"),
        TaskTerminalStatus::Failed if result.timed_out => {
            (AttemptState::TimedOut, "DEADLINE_EXCEEDED")
        }
        TaskTerminalStatus::Failed
            if result.infrastructure_error_code.as_deref() == Some("WORKSPACE_STATE_MISMATCH") =>
        {
            (AttemptState::Failed, "WORKSPACE_SOURCE_PRECONDITION_DRIFT")
        }
        TaskTerminalStatus::Failed
            if result.infrastructure_error_code.as_deref() == Some("INPUT_STATE_MISMATCH") =>
        {
            (AttemptState::Failed, "INPUT_PRECONDITION_DRIFT")
        }
        TaskTerminalStatus::Failed
            if result.infrastructure_error_code.as_deref()
                == Some("HOST_DEPENDENCY_RUNTIME_DRIFT") =>
        {
            (AttemptState::Failed, "HOST_DEPENDENCY_RUNTIME_DRIFT")
        }
        TaskTerminalStatus::Failed
            if result.infrastructure_error_code.as_deref() == Some("EXECUTABLE_RUNTIME_DRIFT") =>
        {
            (AttemptState::Failed, "EXECUTABLE_RUNTIME_DRIFT")
        }
        TaskTerminalStatus::Failed if result.infrastructure_error_code.is_some() => {
            (AttemptState::Failed, "RUNNER_INFRASTRUCTURE_FAILURE")
        }
        TaskTerminalStatus::Failed => (AttemptState::Failed, "PROCESS_EXIT_NONZERO"),
        TaskTerminalStatus::Cancelled => (AttemptState::Cancelled, "STOP_REQUESTED"),
    };
    let infrastructure_error_digest = result
        .infrastructure_error
        .as_deref()
        .map(|message| sha256_bytes(message.as_bytes()));
    let mut artifacts = vec![stdout, stderr];
    artifacts.push(ArtifactRegistration {
        artifact_id: format!("{}.result", current.attempt_id),
        kind: "execution_result".to_string(),
        relative_path: RESULT_FILE.to_string(),
        digest: result_digest.clone(),
        media_type: "application/json".to_string(),
        byte_length: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        truncated: false,
    });
    Ok(TerminalCommit {
        attempt_id: current.attempt_id.clone(),
        expected_row_version: current.row_version,
        state,
        result_digest,
        exit_code: result.exit_code,
        infrastructure_error_digest,
        finished_at_ms: u64::try_from(result.finished_unix_ms).unwrap_or(u64::MAX),
        artifacts,
        reason_code: reason_code.to_string(),
    })
}

fn validate_captured_output(
    attempt: &AttemptRecord,
    output: &CapturedOutput,
    stdout: bool,
) -> RuntimeResult<ArtifactRegistration> {
    let expected_file = if stdout { STDOUT_FILE } else { STDERR_FILE };
    let expected_kind = if stdout { "stdout" } else { "stderr" };
    let expected_id = format!("{}.{}", attempt.attempt_id, expected_kind);
    if output.file_name != expected_file || output.artifact_id != expected_id {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ArtifactIdentityConflict,
            "Runner output identity does not match Attempt",
            Some("artifact"),
            false,
        ));
    }
    let path = Path::new(&attempt.bundle_path).join(expected_file);
    let metadata = fs::metadata(&path).map_err(|error| io_error("inspect output", error))?;
    let digest = sha256_file(&path).map_err(map_universal_error)?;
    if digest != output.digest || metadata.len() != output.retained_bytes {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ArtifactIdentityConflict,
            "Runner output digest or byte length changed",
            Some("artifact"),
            false,
        ));
    }
    Ok(ArtifactRegistration {
        artifact_id: expected_id,
        kind: expected_kind.to_string(),
        relative_path: expected_file.to_string(),
        digest,
        media_type: "text/plain; charset=utf-8".to_string(),
        byte_length: metadata.len(),
        truncated: output.truncated,
    })
}

fn map_universal_error(error: crate::UniversalExecError) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::InvalidRequest,
        error.message,
        error.field.as_deref(),
        error.retryable,
    )
}

fn io_error(context: &str, error: std::io::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::IoError,
        format!("{context}: {error}"),
        None,
        false,
    )
}
